macro_rules! define_roles {
    ( $( $enum:ident => { $( $variant:ident ),* $(,)? } )* ) => { paste::paste! {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash,
                 serde_with::DeserializeFromStr, serde_with::SerializeDisplay,
                 strum::EnumDiscriminants, strum::IntoStaticStr)]
        #[strum(serialize_all = "kebab-case")]
        #[strum_discriminants(derive(strum::IntoStaticStr))]
        #[strum_discriminants(strum(serialize_all = "kebab-case"))]
        pub enum Role {
            $( $enum([< $enum Role >]), )*
        }

        impl Role {
            pub fn iter() -> impl std::iter::Iterator<Item = Role> {
                use strum::IntoEnumIterator;
                itertools::chain!( $( [< $enum Role>]::iter().map(Role::from) ),* )
            }
        }

        impl std::fmt::Display for Role {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(
                        Role::$enum(role) => {
                            let prefix: &'static str = RoleDiscriminants::$enum.into();
                            let suffix = role.as_str();
                            write!(f, "{prefix}-{suffix}")
                        }
                    )*
                }
            }
        }

        impl std::str::FromStr for Role {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                $(
                    let prefix: &'static str = RoleDiscriminants::$enum.into();
                    if s.starts_with(prefix) {
                        $(
                            let suffix: &'static str = [< $enum Role>]::$variant.into();
                            if s.ends_with(suffix) && s.len() == prefix.len() + 1 + suffix.len() {
                                return Ok(Role::$enum([< $enum Role>]::$variant));
                            }
                        )*
                    }
                )*

                    Err(format!("Role not found: {s}"))
            }
        }

        $(
            #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash,
                     strum::EnumIter, strum::IntoStaticStr)]
            #[strum(serialize_all = "kebab-case")]
            pub enum [< $enum Role >] {
                $( $variant, )*
            }

            impl [< $enum Role>] {
                pub fn as_str(self) -> &'static str {
                    self.into()
                }
            }

            impl std::fmt::Display for [< $enum Role>] {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", self.as_str())
                }
            }

            impl From<[< $enum Role >]> for Role {
                fn from(role: [< $enum Role >]) -> Self {
                    Role::$enum(role)
                }
            }

            impl From<[< $enum Role >]> for Roles {
                fn from(role: [< $enum Role >]) -> Self {
                    Roles::One(Role::from(role))
                }
            }

            impl From<[< $enum Role >]> for Access {
                fn from(role: [< $enum Role >]) -> Self {
                    Access::Roles(Roles::from(role))
                }
            }
        )*
    }}
}

macro_rules! define_perms {
    ( $( $enum:ident => { $( $variant:ident ),* $(,)? } )* ) => { paste::paste! {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash,
                 serde_with::DeserializeFromStr, serde_with::SerializeDisplay,
                 strum::EnumDiscriminants, strum::IntoStaticStr)]
        #[strum(serialize_all = "kebab-case")]
        #[strum_discriminants(derive(strum::IntoStaticStr))]
        #[strum_discriminants(strum(serialize_all = "kebab-case"))]
        pub enum Perm {
            $( $enum([< $enum Perm >]), )*
        }

        impl Perm {
            pub fn iter() -> impl std::iter::Iterator<Item = Perm> {
                use strum::IntoEnumIterator;
                itertools::chain!( $( [< $enum Perm>]::iter().map(Perm::from) ),* )
            }
        }

        impl std::fmt::Display for Perm {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(
                        Perm::$enum(role) => {
                            let prefix: &'static str = PermDiscriminants::$enum.into();
                            let suffix = role.as_str();
                            write!(f, "{prefix}-{suffix}")
                        }
                    )*
                }
            }
        }

        impl std::str::FromStr for Perm {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                $(
                    let prefix: &'static str = PermDiscriminants::$enum.into();
                    if s.starts_with(prefix) {
                        $(
                            let suffix: &'static str = [< $enum Perm>]::$variant.into();
                            if s.ends_with(suffix) && s.len() == prefix.len() + 1 + suffix.len() {
                                return Ok(Perm::$enum([< $enum Perm>]::$variant));
                            }
                        )*
                    }
                )*

                    Err(format!("Perm not found: {s}"))
            }
        }

        $(
            #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash,
                     strum::EnumIter, strum::IntoStaticStr)]
            #[strum(serialize_all = "kebab-case")]
            pub enum [< $enum Perm >] {
                $( $variant, )*
            }

            impl [< $enum Perm>] {
                pub fn as_str(self) -> &'static str {
                    self.into()
                }
            }

            impl std::fmt::Display for [< $enum Perm>] {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", self.as_str())
                }
            }

            impl From<[< $enum Perm >]> for Perm {
                fn from(perm: [< $enum Perm >]) -> Self {
                    Perm::$enum(perm)
                }
            }

            impl From<[< $enum Perm >]> for Perms {
                fn from(perm: [< $enum Perm >]) -> Self {
                    Perms::One(Perm::from(perm))
                }
            }

            impl From<[< $enum Perm >]> for Access {
                fn from(perm: [< $enum Perm >]) -> Self {
                    Access::Perms(Perms::from(perm))
                }
            }
        )*
    }}
}
