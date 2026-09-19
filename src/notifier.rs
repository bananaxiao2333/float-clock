//! System-notification scheduling for upcoming moments (with de-duplication and
//! "reloading never re-announces" protection).

use chrono::{Duration, NaiveDateTime};

use crate::config::NotifyConfig;
use crate::notify;
use crate::timefmt::{clock_of, datetime_of, format_hms, format_human};

/// Render notification copy by placeholder; unknown placeholders are kept verbatim.
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
    /// Which moment this reminder belongs to (`target moment` / `offset moment`)
    label: String,
    title: String,
    body: String,
}

/// Build the reminder queue for a set of moments; `tick` fires the ones that are due.
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

    /// (Re)arm. When the moments are unchanged the already-sent records are kept,
    /// so the same reminder never pops up twice.
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
                values.push(("human", "0s".into()));
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

    /// Send the notifications that are due and that became due after this startup;
    /// returns the titles sent this time.
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

    /// The next few reminders that have not fired yet, for the `--print` preview.
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
        notifier.arm(vec![("Target moment".into(), dt(20, 29, 0))], dt(20, 0, 0));
        // 3 before + the exact moment + 2 after
        assert_eq!(notifier.events.len(), 6);
        assert_eq!(notifier.events[0].fire_at, dt(20, 27, 0));
        let upcoming = notifier.upcoming(dt(20, 0, 0), 10);
        assert_eq!(upcoming[0].1, "Target moment");
        assert_eq!(upcoming[0].2, "[T-00:02:00] Target moment");
    }

    #[test]
    fn upcoming_keeps_moments_apart() {
        // The reminders of two moments are interleaved, so the label has to tell
        // them apart: that is what lets `--print` show a few entries per moment
        // instead of letting one moment fill the whole list.
        let mut notifier = notifier();
        notifier.arm(
            vec![
                ("Target moment".into(), dt(20, 29, 0)),
                ("Offset moment".into(), dt(22, 29, 0)),
            ],
            dt(20, 0, 0),
        );
        let upcoming = notifier.upcoming(dt(20, 0, 0), 100);
        let labels: Vec<&str> = upcoming
            .iter()
            .map(|(_, label, _)| label.as_str())
            .collect();
        assert!(labels.contains(&"Target moment"));
        assert!(labels.contains(&"Offset moment"));
        // every entry of one moment renders a distinct title (the title carries
        // the T± clock)
        let titles: Vec<&str> = upcoming
            .iter()
            .filter(|(_, label, _)| label == "Target moment")
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
        // The program only starts at 20:28:30, so the 20:28:00 entry
        // (T-00:01:00) is long gone.
        notifier.arm(vec![("X".into(), dt(20, 29, 0))], dt(20, 28, 30));
        let sent = notifier.tick(dt(20, 28, 30));
        assert!(
            sent.is_empty(),
            "reminders from before startup must not be re-sent: {sent:?}"
        );
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
            ("label", "Offset moment".into()),
            ("sign", "-".into()),
            ("clock", "00:02:00".into()),
            ("human", "2m".into()),
            ("time", "20:29:00".into()),
        ];
        let title = render_copy(&config.title_template, &values);
        let body = render_copy(&config.body_before, &values);
        assert_eq!(title, "[T-00:02:00] Offset moment");
        // The body wording itself lives in config.rs; what matters here is that
        // every placeholder of the shipped template got filled in.
        assert!(body.contains("Offset moment"), "{body}");
        assert!(body.contains("2m"), "{body}");
        assert!(!body.contains('{') && !body.contains('}'), "{body}");
        assert_eq!(
            render_copy("In {human}: {label}", &values),
            "In 2m: Offset moment"
        );
        for text in [&title, &body] {
            for ch in text.chars() {
                let code = ch as u32;
                let is_emoji = matches!(code,
                    0x2190..=0x21FF   // arrows
                    | 0x2300..=0x27BF // symbols and dingbats
                    | 0x2B00..=0x2BFF
                    | 0x1F000..=0x1FAFF
                    | 0xFE0F
                    | 0x200D);
                assert!(
                    !is_emoji,
                    "notification copy must not contain emoji: {ch:?} in {text}"
                );
            }
        }
        assert!(title.contains('[') && title.contains(']'));
    }

    #[test]
    fn unknown_placeholder_is_left_alone() {
        assert_eq!(
            render_copy("{nope} {human}", &[("human", "1m".into())]),
            "{nope} 1m"
        );
    }
}
