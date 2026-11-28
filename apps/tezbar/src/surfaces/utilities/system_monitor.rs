use super::*;
impl Utilities {
    pub(super) fn monitor_action(&mut self, a: Action, cx: &mut Context<Self>) {
        match a {
            Action::MonitorTab(tab) => self.monitor_tab = tab,
            Action::AutoRefresh(enabled) => self.monitor.set_auto_refresh(enabled, self.now()),
            Action::Copy => {
                if let Some(v) = &self.monitor.value {
                    self.request(
                        UtilityRequest::WriteText(v.to_string()),
                        Completion::Simple,
                        false,
                        cx,
                    );
                }
            }
            _ => {}
        }
    }
    pub(super) fn monitor_rows(&self) -> Vec<(String, String)> {
        let query = self.monitor.query.to_lowercase();
        self.monitor_display
            .iter()
            .filter(|r| self.monitor_tab == "overview" || r.section == self.monitor_tab)
            .map(|r| {
                (
                    r.id.clone(),
                    format!("{} · {}: {}", r.section, r.label, r.value),
                )
            })
            .filter(|(_, label)| label.to_lowercase().contains(&query))
            .collect()
    }
}
// SystemMonitorView has manual refresh and a Live toggle, with no process mutation API.

// Layout and copy follow tests/oracles/legacy-renderer/SystemMonitorView.tsx. The existing poll owner
// continues to fence in-flight requests and stop refreshing when the surface is hidden.
impl Utilities {
    pub(super) fn render_monitor(
        &mut self,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut t = tokens::tokens(w);
        if !t.light {
            t.text3 = rgb(0x868a96).into();
        }
        let z = t.zoom;
        let mut header = controls::header(t).px(px(14. * z)).gap(px(10. * z))
            .child(controls::back_button(self.action_button("Back", Action::Cancel, false, cx, t), t))
            .child(div().flex_1().text_size(px(15. * z)).child("System Monitor"))
            .child(self.action_button("Live", Action::AutoRefresh(!self.monitor.auto_refresh), false, cx, t)
                .role(Role::Switch).aria_toggled(if self.monitor.auto_refresh { Toggled::True } else { Toggled::False })
                .h(px(20. * z)).py_0().px(px(7. * z)).rounded(px(6. * z)).text_size(px(11. * z))
                .font_weight(FontWeight::NORMAL).flex_shrink_0().cursor_pointer()
                .text_color(if self.monitor.auto_refresh { t.text1 } else { t.text3 })
                .hover(move |d| d.bg(t.hover))
                .bg(if self.monitor.auto_refresh { t.pill_strong } else { t.pill }))
            .child(self.action_button("Refresh", Action::Refresh, self.monitor.refreshing(), cx, t)
                .text_size(px(0.)).size(px(28. * z)).p_0().flex().items_center().justify_center()
                .when(self.monitor.refreshing(), |d| d.opacity(0.4))
                .child(svg().data(b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"black\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M20 7v5h-5M4 17v-5h5M6 6a8 8 0 0 1 13 2l1 4M4 12l1 4a8 8 0 0 0 13 2\"/></svg>" as &[u8])
                    .size(px(14. * z)).text_color(t.text3)));
        // Do not let a narrow window collapse the two controls into the title.
        header = header.min_w_0();
        let value = self.monitor.value.as_ref();
        let name = value
            .map(|v| monitor_text(&v["device"]["name"]))
            .filter(|v| v != "—")
            .unwrap_or_else(|| "Reading this computer…".into());
        let summary = value
            .map(|v| {
                ["chip", "model", "osVersion"]
                    .iter()
                    .filter_map(|key| v["device"][key].as_str())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ")
            })
            .unwrap_or_else(|| "Hardware and live activity".into());
        let mut body = div()
            .id("system-monitor-dashboard")
            .debug_selector(|| "system-monitor-dashboard".into())
            .role(Role::Group)
            .aria_label("System information")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(14. * z))
            .pb(px(38. * z))
            .child(
                div()
                    .mb(px(12. * z))
                    .child(
                        div()
                            .text_size(px(15. * z))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(name),
                    )
                    .child(
                        div()
                            .mt(px(2. * z))
                            .text_size(px(10.5 * z))
                            .text_color(t.text4)
                            .truncate()
                            .child(summary),
                    ),
            );
        if let Some(v) = value {
            body = body.child(self.monitor_dashboard(v, t));
        } else if let Some(error) = &self.monitor.error {
            body = body.child(
                div()
                    .id("monitor-error")
                    .debug_selector(|| "monitor-error".into())
                    .flex()
                    .flex_col()
                    .min_h(px(260. * z))
                    .items_center()
                    .justify_center()
                    .text_center()
                    .child(
                        div()
                            .text_size(px(14. * z))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xfecdd3))
                            .child("System information unavailable"),
                    )
                    .child(
                        div()
                            .mt(px(4. * z))
                            .max_w(px(420. * z))
                            .text_size(px(11. * z))
                            .text_color(t.text4)
                            .child(error.clone()),
                    )
                    .child(
                        self.action_button(
                            "Try again",
                            Action::Refresh,
                            self.monitor.refreshing(),
                            cx,
                            t,
                        )
                        .mt(px(12. * z)),
                    ),
            );
        } else {
            let mut skeleton = div()
                .id("monitor-loading")
                .debug_selector(|| "monitor-loading".into())
                .flex()
                .flex_col()
                .gap(px(10. * z))
                .aria_label("Loading system information");
            for _ in 0..3 {
                skeleton =
                    skeleton.child(div().flex().gap(px(10. * z)).children((0..3).map(|_| {
                        div()
                            .flex_1()
                            .h(px(108. * z))
                            .rounded(px(14. * z))
                            .border_1()
                            .border_color(rgba(0xffffff0f))
                            .bg(rgba(0xffffff06))
                    })));
            }
            body = body.child(skeleton);
        }
        let status = value
            .and_then(model::monitor::updated_at)
            .map(|s| format!("Updated {s}"))
            .unwrap_or_else(|| "Collecting…".into());
        controls::sheet(t)
            .id("system-monitor")
            .debug_selector(|| "system-monitor".into())
            .relative()
            .line_height(relative(1.5))
            .role(Role::Group)
            .aria_label("System Monitor")
            .font_family(tokens::font_family(cx, false))
            .track_focus(&self.focus)
            .key_context("Utilities")
            .on_key_down(cx.listener(Self::key))
            .child(header)
            .child(body)
            .child(
                controls::footer(t)
                    .id("monitor-status")
                    .debug_selector(|| "monitor-status".into())
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .px(px(8. * z))
                    .text_size(px(11. * z))
                    .text_color(t.text4)
                    .border_t_1()
                    .border_color(t.hairline)
                    .role(Role::Status)
                    .aria_label("System information status")
                    .child(status),
            )
    }
    fn monitor_dashboard(&self, v: &Value, t: tokens::Tokens) -> Div {
        let z = t.zoom;
        let cpu = monitor_number(&v["cpu"]["usagePercent"]);
        let memory = monitor_number(&v["memory"]["usedBytes"]);
        let memory_total = monitor_number(&v["memory"]["totalBytes"]);
        let battery = v.get("battery").filter(|b| !b.is_null());
        let charge = battery
            .and_then(|b| monitor_number(&b["chargePercent"]))
            .unwrap_or(0.);
        let cpu_detail = format!(
            "{} cores · {:.1}% user · {:.1}% system",
            monitor_text(&v["cpu"]["totalCores"]),
            monitor_number(&v["cpu"]["userPercent"]).unwrap_or(0.),
            monitor_number(&v["cpu"]["systemPercent"]).unwrap_or(0.)
        );
        let memory_detail = format!(
            "of {:.0} GB used · {} available",
            memory_total.unwrap_or(0.) / 1024_f64.powi(3),
            monitor_bytes(&v["memory"]["freeBytes"])
        );
        let battery_detail = battery
            .map(|b| {
                format!(
                    "{} · {}",
                    if b["isCharging"] == true {
                        "Charging"
                    } else if b["isPluggedIn"] == true {
                        "On power"
                    } else {
                        "On battery"
                    },
                    b["condition"].as_str().unwrap_or("Condition unavailable")
                )
            })
            .unwrap_or_else(|| "No internal battery detected".into());
        let top = div()
            .flex()
            .gap(px(10. * z))
            .child(monitor_stat(
                "monitor-stat-cpu",
                "CPU Load",
                cpu.map(|n| format!("{:.0}%", n.round()))
                    .unwrap_or_else(|| "—".into()),
                cpu_detail,
                0x7dd3fc,
                cpu.unwrap_or(0.),
                "CPU load",
                t,
            ))
            .child(monitor_stat(
                "monitor-stat-memory",
                "Memory",
                memory
                    .map(|n| format!("{:.1} GB", n / 1024_f64.powi(3)))
                    .unwrap_or_else(|| "—".into()),
                memory_detail,
                0x6ee7b7,
                memory.unwrap_or(0.) / memory_total.unwrap_or(1.).max(1.) * 100.,
                "Memory used",
                t,
            ))
            .child(monitor_stat(
                "monitor-stat-battery",
                "Battery",
                battery
                    .map(|_| format!("{charge:.0}%"))
                    .unwrap_or_else(|| "Not found".into()),
                battery_detail,
                if charge <= 20. { 0xfda4af } else { 0xfcd34d },
                charge,
                "Battery charge",
                t,
            ));
        let percent = monitor_number(&v["storage"]["usedBytes"]).unwrap_or(0.)
            / monitor_number(&v["storage"]["totalBytes"])
                .unwrap_or(1.)
                .max(1.)
            * 100.;
        let mut storage = monitor_panel("monitor-storage", "Storage", t)
            .flex_grow(1.35)
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(16. * z))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(monitor_label("Storage", t))
                            .child(
                                div()
                                    .mt(px(6. * z))
                                    .text_size(px(17. * z))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!(
                                        "{} free",
                                        monitor_bytes(&v["storage"]["freeBytes"])
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .px(px(8. * z))
                            .py(px(4. * z))
                            .rounded(px(5. * z))
                            .border_1()
                            .border_color(rgba(0xffffff12))
                            .bg(rgba(0xffffff08))
                            .text_size(px(9.5 * z))
                            .text_color(t.text3)
                            .child(
                                v["storage"]["smartStatus"]
                                    .as_str()
                                    .unwrap_or("Internal SSD")
                                    .to_owned(),
                            ),
                    ),
            )
            .child(monitor_meter(percent, "Storage used", t).mt(px(12. * z)));
        // Decimal size, with the exact byte count on its own line (it never fits beside it
        // in a half-width column, so it used to be cut off mid-number).
        let exact = |label: &'static str, key: &str| {
            let bytes = monitor_number(&v["storage"][key]);
            let (primary, exact) = bytes
                .map(|n| {
                    (
                        model::monitor::decimal_bytes(n),
                        Some(format!("{} bytes", self.presentation.0.count(n.round().max(0.) as usize))),
                    )
                })
                .unwrap_or_else(|| ("—".into(), None));
            monitor_detail_exact(label, primary, exact, t)
        };
        storage = storage.child(
            div()
                .flex()
                .gap(px(20. * z))
                .mt(px(10. * z))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(exact("Free", "freeBytes"))
                        .child(monitor_detail(
                            "Used",
                            monitor_bytes(&v["storage"]["usedBytes"]),
                            true,
                            t,
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(exact("Capacity", "totalBytes"))
                        .child(monitor_detail(
                            "Drive",
                            monitor_text(
                                v["storage"]
                                    .get("device")
                                    .filter(|d| !d.is_null())
                                    .unwrap_or(&v["storage"]["name"]),
                            ),
                            true,
                            t,
                        )),
                ),
        );
        let mut health = monitor_panel("monitor-battery-health", "Battery Health", t)
            .child(monitor_label("Battery Health", t));
        if let Some(b) = battery {
            let remaining = monitor_count(&b["timeRemainingMinutes"])
                .filter(|n| *n > 0)
                .map(|n| format!("{}h {}m", n / 60, n % 60))
                .unwrap_or_else(|| "Calculating…".into());
            let rows = [
                (
                    "Maximum capacity",
                    monitor_number(&b["maximumCapacityPercent"])
                        .map(|n| format!("{n:.0}%"))
                        .unwrap_or_else(|| "—".into()),
                ),
                ("Cycle count", monitor_text(&b["cycleCount"])),
                ("Condition", monitor_text(&b["condition"])),
                (
                    "Temperature",
                    monitor_number(&b["temperatureCelsius"])
                        .map(|n| format!("{n:.1} °C"))
                        .unwrap_or_else(|| "—".into()),
                ),
                ("Remaining", remaining),
                (
                    "Low Power Mode",
                    if b["lowPowerMode"] == true {
                        "On"
                    } else {
                        "Off"
                    }
                    .into(),
                ),
            ];
            health = health.child(monitor_details(rows, t).mt(px(6. * z)));
        } else {
            health = health.child(
                div()
                    .mt(px(12. * z))
                    .text_size(px(11. * z))
                    .text_color(t.text4)
                    .child("No battery detected."),
            );
        }
        let processor = monitor_panel("monitor-processor", "Processor & GPU", t)
            .child(monitor_label("Processor & GPU", t))
            .child(
                monitor_details(
                    [
                        ("CPU", monitor_text(&v["cpu"]["model"])),
                        (
                            "Core layout",
                            monitor_count(&v["cpu"]["performanceCores"])
                                .zip(monitor_count(&v["cpu"]["efficiencyCores"]))
                                .filter(|(p, e)| *p > 0 && *e > 0)
                                .map(|(p, e)| format!("{p} performance + {e} efficiency"))
                                .unwrap_or_else(|| {
                                    format!("{} cores", monitor_text(&v["cpu"]["totalCores"]))
                                }),
                        ),
                        (
                            "Load average",
                            if v["device"]["platform"] == "Windows" {
                                "Not reported by Windows".into()
                            } else {
                                v["cpu"]["loadAverage"]
                                    .as_array()
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(monitor_number)
                                            .map(|n| format!("{n:.2}"))
                                            .collect::<Vec<_>>()
                                            .join(" · ")
                                    })
                                    .unwrap_or_else(|| "—".into())
                            },
                        ),
                        (
                            "GPU",
                            format!(
                                "{}{}",
                                monitor_text(&v["gpu"]["model"]),
                                monitor_count(&v["gpu"]["cores"])
                                    .filter(|n| *n > 0)
                                    .map(|n| format!(" · {n} cores"))
                                    .unwrap_or_default()
                            ),
                        ),
                        ("Display", monitor_text(&v["gpu"]["display"])),
                    ],
                    t,
                )
                .mt(px(6. * z)),
            );
        let windows = v["device"]["platform"] == "Windows";
        let network = monitor_panel("monitor-network", "Network Activity", t)
            .child(monitor_label("Network Activity", t))
            .child(
                div()
                    .flex()
                    .gap(px(8. * z))
                    .mt(px(8. * z))
                    .child(monitor_rate_card(
                        "Download",
                        model::monitor::rate(monitor_number(
                            &v["network"]["downloadBytesPerSecond"],
                        )),
                        0x6ee7b7,
                        0xa7f3d0,
                        t,
                    ))
                    .child(monitor_rate_card(
                        "Upload",
                        model::monitor::rate(monitor_number(&v["network"]["uploadBytesPerSecond"])),
                        0x7dd3fc,
                        0xbae6fd,
                        t,
                    )),
            )
            .child(
                monitor_details(
                    [
                        (
                            "Connection",
                            v["network"]["connectionType"]
                                .as_str()
                                .or_else(|| v["network"]["interface"].as_str())
                                .unwrap_or("Offline")
                                .into(),
                        ),
                        ("Local IP", monitor_text(&v["network"]["localIp"])),
                        (
                            "Wi-Fi",
                            v["network"]["ssid"]
                                .as_str()
                                .unwrap_or(if windows {
                                    "Not connected or unavailable"
                                } else {
                                    "Name hidden by macOS"
                                })
                                .into(),
                        ),
                    ],
                    t,
                )
                .mt(px(6. * z)),
            );
        let fans = v["fans"]["rpms"]
            .as_array()
            .filter(|a| !a.is_empty())
            .map(|a| {
                a.iter()
                    .filter_map(monitor_count)
                    .map(|n| format!("{} RPM", self.presentation.0.count(n as usize)))
                    .collect::<Vec<_>>()
                    .join(" · ")
            })
            .unwrap_or_else(|| monitor_text(&v["fans"]["status"]));
        let thermals = monitor_panel("monitor-thermals", "Thermals & Activity", t)
            .child(monitor_label("Thermals & Activity", t))
            .child(
                monitor_details(
                    [
                        ("Fans", fans),
                        (
                            "Processes",
                            format!(
                                "{} total · {} running",
                                monitor_text(&v["processes"]["total"]),
                                monitor_text(&v["processes"]["running"])
                            ),
                        ),
                        (
                            "Uptime",
                            monitor_number(&v["device"]["uptimeSeconds"])
                                .map(model::monitor::uptime)
                                .unwrap_or_else(|| "—".into()),
                        ),
                        ("Compressed", monitor_bytes(&v["memory"]["compressedBytes"])),
                        ("Swap used", monitor_bytes(&v["memory"]["swapUsedBytes"])),
                    ],
                    t,
                )
                .mt(px(6. * z)),
            )
            .when_some(
                v["fans"]["note"].as_str().filter(|s| !s.is_empty()),
                |d, note| {
                    d.child(
                        div()
                            .mt(px(8. * z))
                            .text_size(px(9.5 * z))
                            .text_color(t.text4)
                            .child(note.to_owned()),
                    )
                },
            );
        div()
            .flex()
            .flex_col()
            .gap(px(10. * z))
            .child(top)
            .child(div().flex().gap(px(10. * z)).child(storage).child(health))
            .child(
                div()
                    .flex()
                    .gap(px(10. * z))
                    .child(processor)
                    .child(network)
                    .child(thermals),
            )
            .when_some(self.monitor.error.as_ref(), |d, error| {
                d.child(
                    div()
                        .px(px(4. * z))
                        .text_size(px(10. * z))
                        .text_color(rgb(0xfde68a))
                        .child(format!("Refresh warning: {error}")),
                )
            })
    }
}
fn monitor_number(v: &Value) -> Option<f64> {
    v.as_f64().filter(|n| n.is_finite())
}
// Native system-stats values pass through a JS-compatible JSON bridge, so integer
// measurements may arrive as 15.0 as well as 15.
fn monitor_count(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| {
        monitor_number(v)
            .filter(|n| *n >= 0. && n.fract() == 0. && *n < u64::MAX as f64)
            .map(|n| n as u64)
    })
}
fn monitor_text(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .or_else(|| monitor_number(v).map(|n| n.to_string()))
        .unwrap_or_else(|| "—".into())
}
fn monitor_bytes(v: &Value) -> String {
    monitor_number(v)
        .map(model::monitor::decimal_bytes)
        .unwrap_or_else(|| "—".into())
}
fn monitor_label(title: &'static str, t: tokens::Tokens) -> Div {
    div()
        .text_size(px(11. * t.zoom))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.text4)
        .child(title)
}
fn monitor_panel(id: &'static str, title: &'static str, t: tokens::Tokens) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .role(Role::Group)
        .aria_label(title)
        .flex_1()
        .min_w_0()
        .rounded(px(14. * t.zoom))
        .border_1()
        .border_color(rgba(0xffffff12))
        .bg(rgba(0x0000001c))
        .p(px(14. * t.zoom))
}
fn monitor_meter(value: f64, name: &'static str, t: tokens::Tokens) -> Stateful<Div> {
    let value = if value.is_finite() {
        value.clamp(0., 100.)
    } else {
        0.
    };
    div()
        .id(SharedString::from(format!("meter-{name}")))
        .role(Role::ProgressIndicator)
        .aria_label(format!("{name}: {value:.0}%"))
        .h(px(6. * t.zoom))
        .w_full()
        .rounded_full()
        .overflow_hidden()
        .bg(rgba(0x00000040))
        .child(
            div()
                .h_full()
                .w(relative((value / 100.) as f32))
                .rounded_full()
                .bg(rgb(if value >= 85. {
                    0xfb7185
                } else if value >= 65. {
                    0xfcd34d
                } else {
                    0x6ee7b7
                })),
        )
}
#[allow(clippy::too_many_arguments)]
fn monitor_stat(
    id: &'static str,
    title: &'static str,
    value: String,
    detail: String,
    accent: u32,
    percent: f64,
    meter: &'static str,
    t: tokens::Tokens,
) -> Stateful<Div> {
    monitor_panel(id, title, t)
        .border_color(rgba((accent << 8) | 0x33))
        .bg(rgba((accent << 8) | 0x0e))
        .child(monitor_label(title, t))
        .child(
            div()
                .mt(px(6. * t.zoom))
                .line_height(px(22. * t.zoom))
                .text_size(px(22. * t.zoom))
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(value),
        )
        .child(
            div()
                .mt(px(6. * t.zoom))
                .min_h(px(16. * t.zoom))
                .text_size(px(11. * t.zoom))
                .text_color(t.text3)
                .truncate()
                .child(detail),
        )
        .child(monitor_meter(percent, meter, t).mt(px(12. * t.zoom)))
}
fn monitor_detail(label: &'static str, value: String, last: bool, t: tokens::Tokens) -> Div {
    div()
        .flex()
        .min_w_0()
        .items_center()
        .justify_between()
        .gap(px(12. * t.zoom))
        .py(px(8. * t.zoom))
        .when(!last, |d| d.border_b_1().border_color(rgba(0xffffff0b)))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(10.5 * t.zoom))
                .text_color(t.text4)
                .child(label),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_right()
                .text_size(px(11. * t.zoom))
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.text2)
                .child(value),
        )
}
fn monitor_detail_exact(label: &'static str, value: String, exact: Option<String>, t: tokens::Tokens) -> Div {
    div()
        .flex()
        .min_w_0()
        .items_center()
        .justify_between()
        .gap(px(12. * t.zoom))
        .py(px(8. * t.zoom))
        .border_b_1()
        .border_color(rgba(0xffffff0b))
        .child(div().flex_shrink_0().text_size(px(10.5 * t.zoom)).text_color(t.text4).child(label))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .items_end()
                .child(
                    div()
                        .truncate()
                        .text_size(px(11. * t.zoom))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.text2)
                        .child(value),
                )
                .when_some(exact, |d, exact| {
                    d.child(div().truncate().text_size(px(9.5 * t.zoom)).text_color(t.text4).child(exact))
                }),
        )
}
fn monitor_details<const N: usize>(rows: [(&'static str, String); N], t: tokens::Tokens) -> Div {
    div().children(
        rows.into_iter()
            .enumerate()
            .map(|(i, (label, value))| monitor_detail(label, value, i + 1 == N, t)),
    )
}
fn monitor_rate_card(
    title: &'static str,
    value: String,
    tint: u32,
    color: u32,
    t: tokens::Tokens,
) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .rounded(px(9. * t.zoom))
        .border_1()
        .border_color(rgba((tint << 8) | 0x1a))
        .bg(rgba((tint << 8) | 0x0b))
        .p(px(8. * t.zoom))
        .child(
            div()
                .text_size(px(9. * t.zoom))
                .text_color(t.text4)
                .child(title),
        )
        .child(
            div()
                .mt(px(4. * t.zoom))
                .text_size(px(14. * t.zoom))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(color))
                .child(value),
        )
}

#[cfg(test)]
mod numeric_wire_tests {
    use super::{monitor_count, monitor_text};
    use ::core::prelude::v1::test;
    use serde_json::json;
    #[test]
    fn native_json_integer_measurements_accept_float_numbers() {
        for number in [json!(15), json!(15.0), json!(102.0)] {
            assert_ne!(monitor_text(&number), "—");
            assert!(monitor_count(&number).is_some());
        }
        assert_eq!(monitor_text(&json!(15.0)), "15");
        assert_eq!(monitor_count(&json!(102.0)), Some(102));
        assert_eq!(monitor_count(&json!(2.5)), None);
        assert_eq!(monitor_count(&json!(-1)), None);
        assert_eq!(monitor_text(&json!(null)), "—");
    }
}
