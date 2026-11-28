//! Native chat shares the app's core and request admission owner.
use crate::{
    core_client::{CoreClient, Request},
    surfaces::ai_chat::{ChatFuture, ChatRequest, ChatTransport},
};
impl ChatTransport for CoreClient {
    fn send(&self, request: ChatRequest) -> ChatFuture {
        let core = self.clone();
        Box::pin(async move { core.request(Request::Chat(request)).await })
    }
}
