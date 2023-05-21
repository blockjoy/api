#[derive(Clone, Debug)]
pub struct UnauthenticatedPaths {
    urls: Vec<&'static str>,
}

impl UnauthenticatedPaths {
    pub fn new(urls: Vec<&'static str>) -> Self {
        Self { urls }
    }

    pub fn is_unauthenticated(&self, url: &str) -> bool {
        self.urls.contains(&url)
    }
}

#[cfg(test)]
mod tests {
    use crate::auth::unauthenticated_paths::UnauthenticatedPaths;

    #[test]
    fn should_return_true_for_existing_uri() {
        let unauth_paths = UnauthenticatedPaths::new(vec!["/lorem/ipsum"]);

        assert!(unauth_paths.is_unauthenticated("/lorem/ipsum"))
    }

    #[test]
    fn should_return_false_for_not_existing_uri() {
        let unauth_paths = UnauthenticatedPaths::new(vec!["/lorem/ipsum"]);

        assert!(!unauth_paths.is_unauthenticated("/foo/bar"))
    }
}
