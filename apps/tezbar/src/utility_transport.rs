//! The utility views share the existing core owner; this adapter creates no runtime or store.
use crate::{
    core_client::{CoreClient, Request},
    surfaces::utilities::{Reply, UtilityRequest, UtilityTransport},
};

impl UtilityTransport for CoreClient {
    fn send(
        &self,
        request: UtilityRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send>> {
        let core = self.clone();
        Box::pin(async move { core.request(Request::Utility(request)).await })
    }
    fn release(
        &self,
        url: String,
        accept: &'static str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>> {
        let core = self.clone();
        Box::pin(async move { core.release_notes(url, accept).await })
    }
}
