use super::*;
use std::collections::HashSet;
fn tokens(v: &str) -> HashSet<String> {
    v.to_lowercase()
        .split_whitespace()
        .map(|t| {
            t.chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
                .collect::<String>()
        })
        .filter(|t| t.len() > 2)
        .collect()
}
impl AiCore {
    fn answer_event(&self, id: &str, channel: &str, value: Value) {
        // These legacy events have no run id in their payload. A superseded
        // task must never append to, or complete, the replacement response.
        let state = self.inner.state.lock().unwrap();
        if state.answer.as_ref().is_some_and(|r| r.id == id) {
            self.inner.ctx.emit(channel, value);
        }
    }

    pub(super) fn answer(&self, text: String) {
        let cancel = Cancel::new();
        let id = uuid::Uuid::new_v4().to_string();
        {
            let mut state = self.inner.state.lock().unwrap();
            if let Some(old) = state.answer.take() {
                old.cancel.cancel();
            }
            state.answer = Some(Run {
                id: id.clone(),
                cancel: cancel.clone(),
                emitted: false,
            });
        }
        let core = self.clone();
        self.spawn(async move{let result=async{let cfg=core.cfg(Some("chat"),None).await?;let mut messages=vec![json!({"role":"system","content":"You are Hermes, a helpful assistant. Answer briefly and clearly unless the user asks for more detail."})];if cfg["memoryEnabled"]==true{let path=core.inner.ctx.paths.data_dir.join("llm/memory.json");let query=tokens(&text);let max=cfg["memoryMaxItems"].as_u64().unwrap_or(3).min(500)as usize;let private=cfg["memoryIncludePrivate"]==true;let entries=tokio::task::spawn_blocking(move||{let raw=common::read_json(&path).unwrap_or(Value::Null);let mut entries=raw["entries"].as_array().cloned().unwrap_or_default().into_iter().filter(|entry|private||entry["private"]!=true).filter_map(|entry|{let terms=tokens(s(&entry["text"]));let score=query.intersection(&terms).count();(score>0).then_some((score,entry))}).collect::<Vec<_>>();entries.sort_by(|a,b|b.0.cmp(&a.0).then_with(||b.1["createdAt"].as_u64().cmp(&a.1["createdAt"].as_u64())));entries.into_iter().take(max).map(|(_,e)|s(&e["text"]).to_string()).collect::<Vec<_>>()}).await.map_err(err)?;if !entries.is_empty(){messages.push(json!({"role":"system","content":format!("Relevant memory:\n{}",entries.iter().enumerate().map(|(i,s)|format!("{}. {s}",i+1)).collect::<Vec<_>>().join("\n"))}));}}messages.push(json!({"role":"user","content":text}));let answer=core.stream(&cfg,messages,&cancel,|delta|{if let Some(text)=delta["text"].as_str(){core.answer_event(&id,"stream-token",json!(text));}}).await?;if cfg["memoryEnabled"]==true&&!cancel.is_cancelled(){let memory_text=redact_text(&format!("User: {text}\nAssistant: {}",cut(&answer,1200)));let path=core.inner.ctx.paths.data_dir.join("llm/memory.json");tokio::task::spawn_blocking(move||{let mut raw=match common::read_json(&path){Ok(raw)=>raw,Err(e)if path.exists()=>return Err(e),Err(_)=>json!({"entries":[]})};let mut entries=raw["entries"].as_array().cloned().unwrap_or_default();entries.insert(0,json!({"id":format!("mem:{}:{}",now_ms(),uuid::Uuid::new_v4()),"text":memory_text,"source":"conversation","createdAt":now_ms(),"private":false}));entries.truncate(500);raw["entries"]=json!(entries);common::write_json_atomic(&path,&raw)}).await.map_err(err)??;}Ok::<(),String>(())}.await;if let Err(error)=result{if !cancel.is_cancelled(){core.answer_event(&id,"stream-error",json!(error));}}core.answer_event(&id,"stream-done",Value::Null);let mut state=core.inner.state.lock().unwrap();if state.answer.as_ref().is_some_and(|r|r.id==id){state.answer=None;}});
    }
}
