use blockvisor_api::auth::*;
use blockvisor_api::is_owned_by;

struct Resource;
struct Owner;
struct NoOwner;
struct Repo;

#[tonic::async_trait]
impl Owned<Owner, ()> for Resource {
    async fn is_owned_by(&self, _resource: Owner, _db: ()) -> bool {
        true
    }
}

#[tonic::async_trait]
impl Owned<Owner, Repo> for Resource {
    async fn is_owned_by(&self, _resource: Owner, _db: Repo) -> bool {
        true
    }
}

#[tonic::async_trait]
impl Owned<NoOwner, ()> for Resource {
    async fn is_owned_by(&self, _resource: NoOwner, _db: ()) -> bool {
        false
    }
}

#[tonic::async_trait]
impl Owned<NoOwner, Repo> for Resource {
    async fn is_owned_by(&self, _resource: NoOwner, _db: Repo) -> bool {
        false
    }
}

#[tokio::test]
async fn is_owned_by_macro_works_without_repo() {
    let resource = Resource;
    let owner = Owner;

    assert!(is_owned_by! { resource => owner });
}

#[tokio::test]
async fn is_not_owned_by_macro_works_without_repo() {
    let resource = Resource;
    let no_owner = NoOwner;

    assert!(!is_owned_by! { resource => no_owner });
}

#[tokio::test]
async fn is_owned_by_macro_works_with_repo() {
    let resource = Resource;
    let owner = Owner;
    let repo = Repo;

    assert!(is_owned_by! { resource => owner, using repo });
}

#[tokio::test]
async fn is_not_owned_by_macro_works_with_repo() {
    let resource = Resource;
    let no_owner = NoOwner;
    let repo = Repo;

    assert!(!is_owned_by! { resource => no_owner, using repo });
}
