use crate::{
    core_client::{CoreClient, Request},
    surfaces::settings::{Reply, SettingsRequest, SettingsTransport},
};
impl SettingsTransport for CoreClient {
    fn send(&self, request: SettingsRequest) -> super::surfaces::settings::SettingsFuture {
        let core = self.clone();
        Box::pin(async move { core.request(Request::Settings(request)).await })
    }
    fn durable_config(&self) -> super::surfaces::settings::SettingsFuture {
        let config = self.0.ctx.config.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || config.read())
                .await
                .map_err(|_| "Could not read Settings".to_string())
                .and_then(|r: Reply| r)
        })
    }
}
