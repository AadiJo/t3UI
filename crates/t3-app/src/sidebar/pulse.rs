//! Tailwind's `animate-pulse` (opacity 1 → .5 → 1 over 2s, `cubic-bezier(.4,0,.6,1)`), driven
//! by one throttled clock.
//!
//! Every pulsing element computes its opacity from the same epoch, so they stay in phase, and the
//! owner ticks a single ~30 fps timer only while something visible pulses ([`PulseClock`]).

use std::{
    sync::LazyLock,
    time::{Duration, Instant},
};

use gpui_kit::{Context, Task};
use t3_ui::tokens::motion::{CONTINUOUS_FRAME, cubic_bezier};

const PERIOD: Duration = Duration::from_secs(2);

static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Opacity of a pulsing element right now.
pub fn pulse_opacity() -> f32 {
    let period = PERIOD.as_secs_f32();
    let phase = (EPOCH.elapsed().as_secs_f32() % period) / period;
    let ease = |t: f32| cubic_bezier(0.4, 0.0, 0.6, 1.0, t);
    if phase < 0.5 {
        1.0 - 0.5 * ease(phase * 2.0)
    } else {
        0.5 + 0.5 * ease((phase - 0.5) * 2.0)
    }
}

/// Re-renders its owner at ~30 fps while [`PulseClock::set_active`] is true.
#[derive(Default)]
pub struct PulseClock {
    ticker: Option<Task<()>>,
}

impl PulseClock {
    /// Starts or stops ticking `cx`'s entity.
    pub fn set_active<T: 'static>(&mut self, active: bool, cx: &mut Context<T>) {
        match (active, self.ticker.is_some()) {
            (true, false) => {
                self.ticker = Some(cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor().timer(CONTINUOUS_FRAME).await;
                        if this.update(cx, |_, cx| cx.notify()).is_err() {
                            break;
                        }
                    }
                }));
            }
            (false, true) => self.ticker = None,
            _ => {}
        }
    }
}
