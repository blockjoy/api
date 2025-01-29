use std::collections::{HashSet, VecDeque};

use argon2::password_hash::{PasswordHasher, SaltString};
use argon2::{Algorithm, Argon2, PasswordHash};
use chrono::{DateTime, Utc};
use diesel::dsl::LeftJoinQuerySource;
use diesel::expression::expression_types::NotSelectable;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel::sql_types::Bool;
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use password_hash::{PasswordVerifier, Salt};
use rand::rngs::OsRng;
use thiserror::Error;
use tracing::warn;
use validator::Validate;

use crate::auth::rbac::{OrgRole, Role};
use crate::auth::resource::{OrgId, UserId};
use crate::database::Conn;
use crate::grpc::{api, Status};
use crate::model::sql;
use crate::util::{NanosUtc, SearchOperator, SortOrder};

use super::org::NewOrg;
use super::schema::{user_roles, users};
use super::Paginate;

pub mod setting;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// User is already confirmed.
    AlreadyConfirmed,
    /// Failed to create new user: {0}
    Create(diesel::result::Error),
    /// Failed to confirm user: {0}
    Confirm(diesel::result::Error),
    /// No user was found to confirm.
    ConfirmNone,
    /// Failed to mark user as deleted: {0}
    Delete(diesel::result::Error),
    /// Failed to delete user billing: {0}
    DeleteBilling(diesel::result::Error),
    /// Failed to find users: {0}
    FindAll(diesel::result::Error),
    /// Failed to find user for email `{0}`: {1}
    FindByEmail(String, diesel::result::Error),
    /// Failed to find user for id `{0}`: {1}
    FindById(UserId, diesel::result::Error),
    /// Failed to find users by ids `{0:?}`: {1}
    FindByIds(HashSet<UserId>, diesel::result::Error),
    /// Failed to find users with role {1} in org {0}: {2}
    FindByOrgRole(OrgId, Role, diesel::result::Error),
    /// Failed to find owner for org {0}: {1}
    FindOwner(OrgId, diesel::result::Error),
    /// Failed to check if user `{0}` is confirmed: {1}
    IsConfirmed(UserId, diesel::result::Error),
    /// Login failed because no email was found.
    LoginEmail,
    /// Missing password hash.
    MissingHash,
    /// User is not confirmed.
    NotConfirmed,
    /// Org {0} has no owner
    NoOwner(OrgId),
    /// User org model error: {0}
    Org(#[from] crate::model::org::Error),
    /// User pagination: {0}
    Paginate(#[from] crate::model::paginate::Error),
    /// Failed to parse password hash: {0}
    ParseHash(password_hash::Error),
    /// Failed to parse Salt: {0}
    ParseSalt(password_hash::Error),
    /// User RBAC error: {0}
    Rbac(#[from] crate::model::rbac::Error),
    /// Failed to update user: {0}
    Update(diesel::result::Error),
    /// Failed to update user `{0}`: {1}
    UpdateId(UserId, diesel::result::Error),
    /// Failed to update password: {0}
    UpdatePassword(diesel::result::Error),
    /// Failed to validate new user: {0}
    ValidateNew(validator::ValidationErrors),
    /// Failed to verify password: {0}
    VerifyPassword(argon2::password_hash::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => {
                Status::already_exists("User already exists.")
            }
            ConfirmNone
            | Delete(NotFound)
            | DeleteBilling(NotFound)
            | FindAll(NotFound)
            | FindByEmail(_, NotFound)
            | FindById(_, NotFound)
            | FindByIds(_, NotFound) => Status::not_found("User not found."),
            AlreadyConfirmed => Status::failed_precondition("Already confirmed."),
            NotConfirmed => Status::failed_precondition("User is not confirmed."),
            LoginEmail | VerifyPassword(_) => Status::forbidden("Invalid email or password."),
            Paginate(err) => err.into(),
            Org(err) => err.into(),
            Rbac(err) => err.into(),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Debug, AsChangeset, Queryable, Selectable)]
#[diesel(treat_none_as_null = false)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub hashword: String,
    pub salt: String,
    pub created_at: DateTime<Utc>,
    pub first_name: String,
    pub last_name: String,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl User {
    pub async fn by_id(id: UserId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        users::table
            .find(id)
            .filter(users::deleted_at.is_null())
            .get_result(conn)
            .await
            .map_err(|err| Error::FindById(id, err))
    }

    pub async fn by_ids(
        user_ids: &HashSet<UserId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        users::table
            .filter(users::id.eq_any(user_ids))
            .filter(users::deleted_at.is_null())
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByIds(user_ids.clone(), err))
    }

    pub async fn by_email(email: &str, conn: &mut Conn<'_>) -> Result<Self, Error> {
        users::table
            .filter(sql::lower(users::email).eq(&email.trim().to_lowercase()))
            .filter(users::deleted_at.is_null())
            .get_result(conn)
            .await
            .map_err(|err| Error::FindByEmail(email.to_lowercase(), err))
    }

    pub async fn by_org_role(
        org_id: OrgId,
        role: Role,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        users::table
            .inner_join(user_roles::table)
            .filter(users::deleted_at.is_null())
            .filter(user_roles::org_id.eq(org_id))
            .filter(user_roles::role.eq(role.to_string()))
            .select(users::all_columns)
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByOrgRole(org_id, role, err))
    }

    pub async fn owner(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let mut owners = users::table
            .inner_join(user_roles::table)
            .filter(users::deleted_at.is_null())
            .filter(user_roles::org_id.eq(org_id))
            .filter(user_roles::role.eq_any([
                Role::Org(OrgRole::Owner).to_string(),
                Role::Org(OrgRole::Personal).to_string(),
            ]))
            .select(users::all_columns)
            .get_results(conn)
            .await
            .map_err(|err| Error::FindOwner(org_id, err))?;

        if owners.len() > 1 {
            warn!("{} owners for org: {org_id}", owners.len());
        }

        owners.pop().ok_or(Error::NoOwner(org_id))
    }

    pub fn verify_password(&self, password: &str) -> Result<(), Error> {
        let hash = PasswordHash {
            algorithm: Algorithm::default().ident(),
            version: None,
            params: Default::default(),
            salt: Some(Salt::from_b64(&self.salt).map_err(Error::ParseSalt)?),
            hash: Some(self.hashword.parse().map_err(Error::ParseHash)?),
        };

        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .map_err(Error::VerifyPassword)
    }

    pub async fn update(&self, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(users::table.find(self.id))
            .set(self)
            .get_result(conn)
            .await
            .map_err(Error::Update)
    }

    pub async fn update_password(
        &self,
        password: &str,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(Error::VerifyPassword)
            .and_then(|h| h.hash.ok_or(Error::MissingHash))?;

        diesel::update(users::table.find(self.id))
            .set((
                users::hashword.eq(hash.to_string()),
                users::salt.eq(salt.as_str()),
            ))
            .get_result(conn)
            .await
            .map_err(Error::UpdatePassword)
    }

    pub async fn login(email: &str, password: &str, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let user = match Self::by_email(email, conn).await {
            Ok(user) => Ok(user),
            Err(Error::FindByEmail(_, NotFound)) => Err(Error::LoginEmail),
            Err(err) => Err(err),
        }?;

        if User::is_confirmed(user.id, conn).await? {
            user.verify_password(password)?;
            Ok(user)
        } else {
            Err(Error::NotConfirmed)
        }
    }

    pub async fn confirm(user_id: UserId, conn: &mut Conn<'_>) -> Result<(), Error> {
        let target_user = users::table
            .find(user_id)
            .filter(users::confirmed_at.is_null())
            .filter(users::deleted_at.is_null());
        let updated = diesel::update(target_user)
            .set(users::confirmed_at.eq(Utc::now()))
            .execute(conn)
            .await
            .map_err(Error::Confirm)?;

        if updated == 0 && Self::is_confirmed(user_id, conn).await? {
            Err(Error::AlreadyConfirmed)
        } else if updated == 0 {
            Err(Error::ConfirmNone)
        } else {
            Ok(())
        }
    }

    pub async fn is_confirmed(id: UserId, conn: &mut Conn<'_>) -> Result<bool, Error> {
        users::table
            .find(id)
            .filter(users::deleted_at.is_null())
            .select(users::confirmed_at.is_not_null())
            .get_result(conn)
            .await
            .map_err(|err| Error::IsConfirmed(id, err))
    }

    pub async fn delete(id: UserId, conn: &mut Conn<'_>) -> Result<(), Error> {
        diesel::update(users::table.find(id))
            .set(users::deleted_at.eq(Utc::now()))
            .execute(conn)
            .await
            .map(|_| ())
            .map_err(Error::Delete)
    }

    pub fn name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
    }
}

pub struct UserSearch {
    pub operator: SearchOperator,
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum UserSort {
    Email(SortOrder),
    FirstName(SortOrder),
    LastName(SortOrder),
    CreatedAt(SortOrder),
}

impl UserSort {
    fn into_expr<T>(self) -> Box<dyn BoxableExpression<T, Pg, SqlType = NotSelectable>>
    where
        users::email: SelectableExpression<T>,
        users::first_name: SelectableExpression<T>,
        users::last_name: SelectableExpression<T>,
        users::created_at: SelectableExpression<T>,
    {
        use SortOrder::*;
        use UserSort::*;

        match self {
            Email(Asc) => Box::new(users::email.asc()),
            Email(Desc) => Box::new(users::email.desc()),

            FirstName(Asc) => Box::new(users::first_name.asc()),
            FirstName(Desc) => Box::new(users::first_name.desc()),

            LastName(Asc) => Box::new(users::last_name.asc()),
            LastName(Desc) => Box::new(users::last_name.desc()),

            CreatedAt(Asc) => Box::new(users::created_at.asc()),
            CreatedAt(Desc) => Box::new(users::created_at.desc()),
        }
    }
}

pub struct UserFilter {
    pub user_ids: Vec<UserId>,
    pub org_ids: Vec<OrgId>,
    pub search: Option<UserSearch>,
    pub sort: VecDeque<UserSort>,
    pub limit: i64,
    pub offset: i64,
}

impl UserFilter {
    pub async fn query(mut self, conn: &mut Conn<'_>) -> Result<(Vec<User>, u64), Error> {
        let mut query = users::table.left_join(user_roles::table).into_boxed();

        if !self.user_ids.is_empty() {
            query = query.filter(users::id.eq_any(self.user_ids));
        }

        if !self.org_ids.is_empty() {
            query = query.filter(user_roles::org_id.eq_any(self.org_ids));
        }

        if let Some(search) = self.search {
            query = query.filter(search.into_expression());
        }

        if let Some(sort) = self.sort.pop_front() {
            query = query.order_by(sort.into_expr());
        } else {
            query = query.order_by(users::created_at.desc());
        }

        while let Some(sort) = self.sort.pop_front() {
            query = query.then_order_by(sort.into_expr());
        }

        query
            .filter(users::deleted_at.is_null())
            .select(User::as_select())
            .distinct()
            .paginate(self.limit, self.offset)?
            .count_results(conn)
            .await
            .map_err(Into::into)
    }
}

type UsersAndRoles = LeftJoinQuerySource<users::table, user_roles::table>;

impl UserSearch {
    fn into_expression(self) -> Box<dyn BoxableExpression<UsersAndRoles, Pg, SqlType = Bool>> {
        match self.operator {
            SearchOperator::Or => {
                let mut predicate: Box<dyn BoxableExpression<UsersAndRoles, Pg, SqlType = Bool>> =
                    Box::new(false.into_sql::<Bool>());
                if let Some(name) = self.name {
                    let full_name = users::first_name.concat(" ").concat(users::last_name);
                    predicate = Box::new(predicate.or(sql::lower(full_name).like(name)));
                }
                if let Some(email) = self.email {
                    predicate = Box::new(predicate.or(sql::lower(users::email).like(email)));
                }
                predicate
            }
            SearchOperator::And => {
                let mut predicate: Box<dyn BoxableExpression<UsersAndRoles, Pg, SqlType = Bool>> =
                    Box::new(true.into_sql::<Bool>());
                if let Some(name) = self.name {
                    let full_name = users::first_name.concat(" ").concat(users::last_name);
                    predicate = Box::new(predicate.and(sql::lower(full_name).like(name)));
                }
                if let Some(email) = self.email {
                    predicate = Box::new(predicate.and(sql::lower(users::email).like(email)));
                }
                predicate
            }
        }
    }
}

#[derive(Clone, Debug, Validate, Insertable)]
#[diesel(table_name = users)]
pub struct NewUser<'a> {
    #[validate(email)]
    email: String,
    first_name: &'a str,
    last_name: &'a str,
    hashword: String,
    salt: String,
}

impl<'a> NewUser<'a> {
    pub fn new(
        email: &'a str,
        first_name: &'a str,
        last_name: &'a str,
        password: &'a str,
    ) -> Result<Self, Error> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(Error::VerifyPassword)
            .and_then(|h| h.hash.ok_or(Error::MissingHash))?;

        let create_user = Self {
            email: email.trim().to_lowercase(),
            first_name,
            last_name,
            hashword: hash.to_string(),
            salt: salt.as_str().to_owned(),
        };

        create_user
            .validate()
            .map(|()| create_user)
            .map_err(Error::ValidateNew)
    }

    pub async fn create(self, conn: &mut Conn<'_>) -> Result<User, Error> {
        let user: User = diesel::insert_into(users::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)?;

        NewOrg::personal().create(user.id, conn).await?;

        Ok(user)
    }
}

#[derive(Debug, Clone, AsChangeset)]
#[diesel(table_name = users)]
pub struct UpdateUser<'a> {
    pub id: UserId,
    pub first_name: Option<&'a str>,
    pub last_name: Option<&'a str>,
}

impl UpdateUser<'_> {
    pub async fn apply(self, conn: &mut Conn<'_>) -> Result<User, Error> {
        let user_id = self.id;
        diesel::update(users::table.find(user_id))
            .set(self)
            .get_result(conn)
            .await
            .map_err(|err| Error::UpdateId(user_id, err))
    }
}

impl From<User> for api::User {
    fn from(user: User) -> Self {
        api::User {
            user_id: user.id.to_string(),
            email: user.email,
            first_name: user.first_name,
            last_name: user.last_name,
            created_at: Some(NanosUtc::from(user.created_at).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn test_password_is_backwards_compatible() {
        let user = User {
            id: Uuid::new_v4().into(),
            email: "shitballer@joe.com".to_string(),
            hashword: "8reOLS3bLZB4vQvqy8Xqoa+mS82d9qidx7j1KTtmICY".to_string(),
            salt: "s2UTzLjLAz4xzhDBTFQtcg".to_string(),
            created_at: chrono::Utc::now(),
            first_name: "Joe".to_string(),
            last_name: "Ballington".to_string(),
            confirmed_at: Some(chrono::Utc::now()),
            deleted_at: None,
        };
        user.verify_password("A password that cannot be hacked!1")
            .unwrap();
    }
}
