use reqwest::Method;

use super::{Api, ApiError, done};

impl Api {
    pub async fn abilities(&self) -> Result<(), ApiError> {
        done(self.request(Method::GET, &["abilities"])).await
    }
}
