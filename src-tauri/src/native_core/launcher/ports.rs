use super::providers::output;
use crate::native_core::common::CoreResult;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
pub async fn list() -> CoreResult {
    #[cfg(windows)]
    {
        let raw = output("netstat.exe", &["-ano", "-p", "tcp"], 8).await?;
        let processes = output("tasklist.exe", &["/fo", "csv", "/nh"], 8)
            .await
            .unwrap_or_default();
        let mut names = HashMap::new();
        for line in processes.lines() {
            let parts: Vec<_> = line.trim_matches('"').split("\",\"").collect();
            if parts.len() >= 2 {
                names.insert(parts[1].to_owned(), parts[0].to_owned());
            }
        }
        return Ok(json!(parse_windows(&raw, &names)));
    }
    #[cfg(not(windows))]
    {
        let raw = match output("/usr/sbin/lsof", &["-nP", "-iTCP", "-sTCP:LISTEN"], 8).await {
            Ok(s) => s,
            Err(e) if e.is_empty() => String::new(),
            Err(e) => return Err(e),
        };
        let ps = output("/bin/ps", &["-axo", "pid=,comm="], 8)
            .await
            .unwrap_or_default();
        let mut names = HashMap::new();
        for line in ps.lines() {
            let line = line.trim();
            if let Some((pid, cmd)) = line.split_once(char::is_whitespace) {
                names.insert(
                    pid.to_owned(),
                    std::path::Path::new(cmd.trim())
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        Ok(json!(parse_lsof(&raw, &names)))
    }
}
fn grouped(entries: Vec<(String, String, String, u16)>) -> Vec<Value> {
    let mut groups: HashMap<(String, String, String), BTreeSet<u16>> = HashMap::new();
    for (name, pid, user, port) in entries {
        groups.entry((name, pid, user)).or_default().insert(port);
    }
    let mut out:Vec<_>=groups.into_iter().map(|((name,pid,user),ports)|json!({"process":name,"pid":pid,"user":user,"ports":ports})).collect();
    out.sort_by(|a, b| {
        a["process"]
            .as_str()
            .cmp(&b["process"].as_str())
            .then_with(|| a["pid"].as_str().cmp(&b["pid"].as_str()))
    });
    out
}
fn decode(s: &str) -> String {
    regex::Regex::new(r"\\x([0-9a-fA-F]{2})")
        .unwrap()
        .replace_all(s, |c: &regex::Captures| {
            u8::from_str_radix(&c[1], 16)
                .map(|n| (n as char).to_string())
                .unwrap_or_default()
        })
        .into_owned()
}
pub fn parse_lsof(raw: &str, names: &HashMap<String, String>) -> Vec<Value> {
    let re = regex::Regex::new(r":(\d+)\s+\(LISTEN\)$").unwrap();
    grouped(
        raw.lines()
            .filter_map(|line| {
                let cols: Vec<_> = line.split_whitespace().collect();
                let port = re.captures(line)?.get(1)?.as_str().parse().ok()?;
                if cols.len() < 3 {
                    return None;
                }
                Some((
                    names
                        .get(cols[1])
                        .cloned()
                        .unwrap_or_else(|| decode(cols[0])),
                    cols[1].into(),
                    cols[2].into(),
                    port,
                ))
            })
            .collect(),
    )
}
#[cfg(any(windows, test))]
pub fn parse_windows(raw: &str, names: &HashMap<String, String>) -> Vec<Value> {
    grouped(
        raw.lines()
            .filter_map(|line| {
                let c: Vec<_> = line.split_whitespace().collect();
                if c.len() < 5 || c[0] != "TCP" || c[3] != "LISTENING" {
                    return None;
                }
                let port = c[1].rsplit(':').next()?.parse().ok()?;
                Some((
                    names.get(c[4]).cloned().unwrap_or("unknown".into()),
                    c[4].into(),
                    "current user".into(),
                    port,
                ))
            })
            .collect(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ipv4_ipv6_dedup() {
        let raw="COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME\nnode 123 dev 4u IPv4 a 0t0 TCP *:3000 (LISTEN)\nnode 123 dev 5u IPv6 b 0t0 TCP [::1]:3000 (LISTEN)\nnode 123 dev 6u IPv4 c 0t0 TCP *:8080 (LISTEN)";
        let r = parse_lsof(raw, &HashMap::new());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0]["ports"], json!([3000, 8080]));
        let r = parse_windows(" TCP [::]:3000 [::]:0 LISTENING 123", &HashMap::new());
        assert_eq!(r[0]["pid"], "123");
    }
}
