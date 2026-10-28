use std::time::{Duration, Instant};

/// Se consulta con un reloj independiente del render: también detecta un mpv sin callbacks.
pub struct Health {
    started: Instant,
    progressed: Instant,
    video_progressed: Instant,
    position: Option<f64>,
    video_position: Option<f64>,
    healthy_since: Option<Instant>,
    failures: u32,
    retry_at: Option<Instant>,
    lag_since: Option<Instant>,
}

impl Health {
    pub fn new(now: Instant) -> Self {
        Self { started: now, progressed: now, video_progressed: now, position: None, video_position: None, healthy_since: None, failures: 0, retry_at: None, lag_since: None }
    }

    pub fn loaded(&mut self, now: Instant) {
        let failures = self.failures;
        *self = Self::new(now);
        self.failures = failures;
    }

    pub fn failed(&mut self, now: Instant) {
        let delay = (5u64 << self.failures.min(4)).min(60);
        self.failures = self.failures.saturating_add(1);
        self.retry_at = Some(now + Duration::from_secs(delay));
        self.healthy_since = None;
    }

    pub fn sample(&mut self, now: Instant, pos: Option<f64>, video: Option<f64>, expect_video: bool, suspended: bool) -> bool {
        if suspended {
            self.started = now;
            self.progressed = now;
            self.video_progressed = now;
            self.position = pos;
            self.video_position = video;
            self.healthy_since = None;
            self.lag_since = None;
            return false;
        }
        if let Some(retry) = self.retry_at {
            return now >= retry;
        }
        let advanced = pos.is_some_and(|p| self.position.is_none_or(|old| (p - old).abs() > 0.01));
        if advanced {
            self.progressed = now;
            let since = *self.healthy_since.get_or_insert(now);
            if now.duration_since(since) >= Duration::from_secs(30) {
                self.failures = 0;
            }
        }
        if video.is_some_and(|p| self.video_position.is_none_or(|old| (p - old).abs() > 0.01)) {
            self.video_progressed = now;
        }
        if !expect_video {
            self.video_progressed = now;
        }
        // Una propiedad temporalmente ausente durante buffering no cuenta como progreso.
        if pos.is_some() { self.position = pos; }
        if video.is_some() { self.video_position = video; }
        let startup = now.duration_since(self.started) >= Duration::from_secs(20);
        let stalled = now.duration_since(self.progressed) >= Duration::from_secs(8)
            && (self.position.is_some() || startup);
        let frozen_video = expect_video && now.duration_since(self.video_progressed) >= Duration::from_secs(8)
            && (self.video_position.is_some() || startup);
        if stalled || frozen_video { self.healthy_since = None; }
        stalled || frozen_video
    }

    pub fn behind(&mut self, now: Instant, buffer: f64, enabled: bool) -> bool {
        if !enabled || buffer < 10.0 || self.retry_at.is_some() {
            self.lag_since = None;
            return false;
        }
        now.duration_since(*self.lag_since.get_or_insert(now)) >= Duration::from_secs(3)
    }

    pub fn ended(&self, now: Instant) -> bool {
        self.retry_at.is_none() && now.duration_since(self.started) >= Duration::from_secs(8)
    }

    /// Aviso antes de la recuperación automática; una pausa intencional reinicia estos relojes.
    pub fn needs_attention(&self, now: Instant) -> bool {
        self.retry_at.is_some()
            || (self.position.is_some() && now.duration_since(self.progressed) >= Duration::from_secs(3))
            || (self.video_position.is_some() && now.duration_since(self.video_progressed) >= Duration::from_secs(3))
            || (self.position.is_none() && now.duration_since(self.started) >= Duration::from_secs(5))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_video_is_detected_even_when_audio_advances() {
        let t = Instant::now();
        let mut h = Health::new(t);
        for s in 0..8 {
            assert!(!h.sample(t + Duration::from_secs(s), Some(s as f64), Some(0.0), true, false));
        }
        assert!(h.sample(t + Duration::from_secs(8), Some(8.0), Some(0.0), true, false));
    }

    #[test]
    fn startup_gets_time_but_missing_properties_cannot_hide_a_stall() {
        let t = Instant::now();
        let mut h = Health::new(t);
        assert!(!h.sample(t + Duration::from_secs(19), None, None, true, false));
        assert!(h.sample(t + Duration::from_secs(20), None, None, true, false));
        h.loaded(t);
        assert!(!h.sample(t, Some(1.0), Some(1.0), true, false));
        assert!(h.sample(t + Duration::from_secs(8), None, None, true, false));
    }

    #[test]
    fn pause_and_audio_only_do_not_trigger_false_reloads() {
        let t = Instant::now();
        let mut h = Health::new(t);
        for s in 0..100 {
            assert!(!h.sample(t + Duration::from_secs(s), Some(1.0), Some(1.0), true, true));
        }
        assert!(!h.sample(t + Duration::from_secs(100), Some(1.0), Some(1.0), true, false));
        for s in 101..150 {
            assert!(!h.sample(t + Duration::from_secs(s), Some(s as f64), None, false, false));
        }
    }

    #[test]
    fn failures_back_off_and_stable_playback_resets_the_delay() {
        let mut t = Instant::now();
        let mut h = Health::new(t);
        for delay in [5, 10, 20, 40, 60, 60] {
            h.failed(t);
            assert!(!h.sample(t + Duration::from_secs(delay - 1), None, None, true, false));
            t += Duration::from_secs(delay);
            assert!(h.sample(t, None, None, true, false));
            h.loaded(t);
        }
        for s in 0..=31 {
            assert!(!h.sample(t + Duration::from_secs(s), Some(s as f64), None, false, false));
        }
        t += Duration::from_secs(32);
        h.failed(t);
        assert!(h.sample(t + Duration::from_secs(5), None, None, false, false));
    }

    #[test]
    fn large_delay_requires_sustained_lag_and_respects_rewind() {
        let t = Instant::now();
        let mut h = Health::new(t);
        assert!(!h.behind(t, 15.0, true));
        assert!(h.behind(t + Duration::from_secs(3), 15.0, true));
        assert!(!h.behind(t + Duration::from_secs(4), 15.0, false));
        assert!(!h.behind(t + Duration::from_secs(5), 15.0, true));
    }

    #[test]
    fn refresh_lights_before_recovery_and_clears_when_frames_resume() {
        let t = Instant::now();
        let mut h = Health::new(t);
        h.sample(t, Some(0.0), Some(0.0), true, false);
        assert!(!h.needs_attention(t + Duration::from_secs(2)));
        assert!(!h.sample(t + Duration::from_secs(3), Some(3.0), Some(0.0), true, false));
        assert!(h.needs_attention(t + Duration::from_secs(3)));
        h.sample(t + Duration::from_secs(4), Some(4.0), Some(4.0), true, false);
        assert!(!h.needs_attention(t + Duration::from_secs(4)));
        h.failed(t + Duration::from_secs(5));
        assert!(h.needs_attention(t + Duration::from_secs(5)));
    }

    #[test]
    fn intentional_pause_does_not_light_refresh() {
        let t = Instant::now();
        let mut h = Health::new(t);
        for s in 0..20 {
            let now = t + Duration::from_secs(s);
            h.sample(now, Some(1.0), Some(1.0), true, true);
            assert!(!h.needs_attention(now));
        }
    }
}
