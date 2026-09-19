//! 临近时间点的系统通知调度（带去重与「重载不重发」保护）。

use chrono::{Duration, NaiveDateTime};

use crate::config::NotifyConfig;
use crate::notify;
use crate::timefmt::{clock_of, datetime_of, format_hms, format_human};

/// 按占位符渲染通知文案；认不出的占位符原样保留。
pub fn render_copy(template: &str, values: &[(&str, String)]) -> String {
    let mut text = template.to_string();
    for (key, value) in values {
        text = text.replace(&format!("{{{key}}}"), value);
    }
    text
}

const GRACE_SECONDS: i64 = 1;

#[derive(Debug, Clone, PartialEq)]
struct Event {
    fire_at: NaiveDateTime,
    key: String,
    /// 这条提醒属于哪个时间点（`目标时间点` / `偏移时刻`）
    label: String,
    title: String,
    body: String,
}

/// 为若干「时间点」排好提醒队列，tick 时把到点的发出去。
pub struct MomentNotifier {
    config: NotifyConfig,
    moments: Vec<(String, NaiveDateTime)>,
    events: Vec<Event>,
    fired: Vec<String>,
    armed_at: Option<NaiveDateTime>,
}

impl MomentNotifier {
    pub fn new(config: NotifyConfig) -> Self {
        Self {
            config,
            moments: Vec::new(),
            events: Vec::new(),
            fired: Vec::new(),
            armed_at: None,
        }
    }

    pub fn config_mut(&mut self) -> &mut NotifyConfig {
        &mut self.config
    }

    /// （重新）布防。时间点没变化时保留已发送记录，避免重复弹窗。
    pub fn arm(&mut self, moments: Vec<(String, NaiveDateTime)>, now: NaiveDateTime) {
        if moments == self.moments {
            self.armed_at = Some(now);
            return;
        }

        self.moments = moments.clone();
        self.fired.clear();
        self.armed_at = Some(now);

        let mut events: Vec<Event> = Vec::new();
        for (label, moment) in &moments {
            let base: Vec<(&str, String)> = vec![
                ("label", label.clone()),
                ("time", clock_of(*moment)),
                ("datetime", datetime_of(*moment)),
            ];

            let mut leads: Vec<i64> = self
                .config
                .before
                .iter()
                .copied()
                .filter(|v| *v > 0)
                .collect();
            leads.sort_unstable();
            leads.dedup();
            for lead in leads {
                let mut values = base.clone();
                values.push(("sign", "-".into()));
                values.push(("clock", format_hms(lead, true)));
                values.push(("human", format_human(lead as f64)));
                events.push(Event {
                    fire_at: *moment - Duration::seconds(lead),
                    key: format!("{label}|-{lead}"),
                    label: label.clone(),
                    title: render_copy(&self.config.title_template, &values),
                    body: render_copy(&self.config.body_before, &values),
                });
            }

            if self.config.at_moment {
                let mut values = base.clone();
                values.push(("sign", "-".into()));
                values.push(("clock", format_hms(0, true)));
                values.push(("human", "0 秒".into()));
                events.push(Event {
                    fire_at: *moment,
                    key: format!("{label}|0"),
                    label: label.clone(),
                    title: render_copy(&self.config.title_template, &values),
                    body: render_copy(&self.config.body_at, &values),
                });
            }

            let mut trails: Vec<i64> = self
                .config
                .after
                .iter()
                .copied()
                .filter(|v| *v > 0)
                .collect();
            trails.sort_unstable();
            trails.dedup();
            for trail in trails {
                let mut values = base.clone();
                values.push(("sign", "+".into()));
                values.push(("clock", format_hms(trail, true)));
                values.push(("human", format_human(trail as f64)));
                events.push(Event {
                    fire_at: *moment + Duration::seconds(trail),
                    key: format!("{label}|+{trail}"),
                    label: label.clone(),
                    title: render_copy(&self.config.title_template, &values),
                    body: render_copy(&self.config.body_after, &values),
                });
            }
        }

        events.sort_by_key(|event| event.fire_at);
        self.events = events;
    }

    /// 发送到点、且本次启动之后到点的通知；返回本次发出的标题。
    pub fn tick(&mut self, now: NaiveDateTime) -> Vec<String> {
        if !self.config.enabled {
            return Vec::new();
        }
        let Some(armed_at) = self.armed_at else {
            return Vec::new();
        };
        let cutoff = armed_at - Duration::seconds(GRACE_SECONDS);
        let sound = if self.config.sound {
            Some(self.config.sound_name.clone())
        } else {
            None
        };
        let mut sent = Vec::new();
        for event in &self.events {
            if self.fired.contains(&event.key) {
                continue;
            }
            if now >= event.fire_at && event.fire_at >= cutoff {
                self.fired.push(event.key.clone());
                notify::send_async(&event.title, &event.body, sound.as_deref());
                sent.push(event.title.clone());
            }
        }
        sent
    }

    /// 还没发的最近若干条提醒，供 `--print` 预览。
    pub fn upcoming(
        &self,
        now: NaiveDateTime,
        limit: usize,
    ) -> Vec<(NaiveDateTime, String, String)> {
        self.events
            .iter()
            .filter(|event| !self.fired.contains(&event.key) && event.fire_at >= now)
            .take(limit)
            .map(|event| (event.fire_at, event.label.clone(), event.title.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(hh: u32, mm: u32, ss: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 9, 19)
            .unwrap()
            .and_hms_opt(hh, mm, ss)
            .unwrap()
    }

    fn notifier() -> MomentNotifier {
        MomentNotifier::new(NotifyConfig {
            before: vec![120, 60, 10],
            after: vec![1, 60],
            at_moment: true,
            ..Default::default()
        })
    }

    #[test]
    fn arms_expected_number_of_events() {
        let mut notifier = notifier();
        notifier.arm(vec![("目标时间点".into(), dt(20, 29, 0))], dt(20, 0, 0));
        // 3 个 before + 正点 + 2 个 after
        assert_eq!(notifier.events.len(), 6);
        assert_eq!(notifier.events[0].fire_at, dt(20, 27, 0));
        let upcoming = notifier.upcoming(dt(20, 0, 0), 10);
        assert_eq!(upcoming[0].1, "目标时间点");
        assert_eq!(upcoming[0].2, "[T-00:02:00] 目标时间点");
    }

    #[test]
    fn upcoming_keeps_moments_apart() {
        // 两个时刻的提醒交错排在一起，label 必须能区分开，
        // `--print` 靠它才能做到「每个时刻各显示几条」而不是被一个时刻占满。
        let mut notifier = notifier();
        notifier.arm(
            vec![
                ("目标时间点".into(), dt(20, 29, 0)),
                ("偏移时刻".into(), dt(22, 29, 0)),
            ],
            dt(20, 0, 0),
        );
        let upcoming = notifier.upcoming(dt(20, 0, 0), 100);
        let labels: Vec<&str> = upcoming
            .iter()
            .map(|(_, label, _)| label.as_str())
            .collect();
        assert!(labels.contains(&"目标时间点"));
        assert!(labels.contains(&"偏移时刻"));
        // 同一个时刻的所有条目，渲染出来的标题互不相同（标题里带 T± 时钟）
        let titles: Vec<&str> = upcoming
            .iter()
            .filter(|(_, label, _)| label == "目标时间点")
            .map(|(_, _, title)| title.as_str())
            .collect();
        let mut unique = titles.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), titles.len());
    }

    #[test]
    fn tick_fires_only_once() {
        let mut notifier = notifier();
        notifier.arm(vec![("X".into(), dt(20, 29, 0))], dt(20, 0, 0));
        assert!(!notifier.tick(dt(20, 27, 0)).is_empty());
        assert!(notifier.tick(dt(20, 27, 0)).is_empty());
        assert!(notifier.tick(dt(20, 27, 1)).is_empty());
    }

    #[test]
    fn does_not_fire_events_older_than_startup() {
        let mut notifier = notifier();
        // 程序在 20:28:30 才启动，20:28:00 那条（T-00:01:00）早就过去了
        notifier.arm(vec![("X".into(), dt(20, 29, 0))], dt(20, 28, 30));
        let sent = notifier.tick(dt(20, 28, 30));
        assert!(sent.is_empty(), "不应补发启动前的提醒：{sent:?}");
    }

    #[test]
    fn rearm_same_moments_keeps_fired_state() {
        let mut notifier = notifier();
        let moments = vec![("X".into(), dt(20, 29, 0))];
        notifier.arm(moments.clone(), dt(20, 0, 0));
        notifier.tick(dt(20, 27, 0));
        let before = notifier.fired.len();
        notifier.arm(moments, dt(20, 27, 1));
        assert_eq!(notifier.fired.len(), before);
        assert!(notifier.tick(dt(20, 27, 2)).is_empty());
    }

    #[test]
    fn rearm_changed_moments_resets() {
        let mut notifier = notifier();
        notifier.arm(vec![("X".into(), dt(20, 29, 0))], dt(20, 0, 0));
        notifier.tick(dt(20, 27, 0));
        assert!(!notifier.fired.is_empty());
        notifier.arm(vec![("X".into(), dt(21, 0, 0))], dt(20, 30, 0));
        assert!(notifier.fired.is_empty());
    }

    #[test]
    fn disabled_notifier_is_silent() {
        let mut notifier = notifier();
        notifier.config.enabled = false;
        notifier.arm(vec![("X".into(), dt(20, 29, 0))], dt(20, 0, 0));
        assert!(notifier.tick(dt(20, 29, 0)).is_empty());
    }

    #[test]
    fn copy_has_no_emoji_and_uses_brackets() {
        let config = NotifyConfig::default();
        let values: Vec<(&str, String)> = vec![
            ("label", "偏移时刻".into()),
            ("sign", "-".into()),
            ("clock", "00:02:00".into()),
            ("human", "2 分".into()),
            ("time", "20:29:00".into()),
        ];
        let title = render_copy(&config.title_template, &values);
        let body = render_copy(&config.body_before, &values);
        assert_eq!(title, "[T-00:02:00] 偏移时刻");
        assert_eq!(body, "距离偏移时刻还有 2 分");
        for text in [&title, &body] {
            for ch in text.chars() {
                let code = ch as u32;
                let is_emoji = matches!(code,
                    0x2190..=0x21FF   // 箭头
                    | 0x2300..=0x27BF // 各种符号、装饰符
                    | 0x2B00..=0x2BFF
                    | 0x1F000..=0x1FAFF
                    | 0xFE0F
                    | 0x200D);
                assert!(!is_emoji, "通知文案里不应出现 emoji：{text} 里的 {ch:?}");
            }
        }
        assert!(title.contains('[') && title.contains(']'));
    }

    #[test]
    fn unknown_placeholder_is_left_alone() {
        assert_eq!(
            render_copy("{nope} {human}", &[("human", "1 分".into())]),
            "{nope} 1 分"
        );
    }
}
