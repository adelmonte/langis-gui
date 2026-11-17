//! Touchpad scrolling as Chromium does it: faster than egui's one to one,
//! and with momentum, a fling that keeps going and slows down once the
//! fingers lift. egui stops dead when the events stop.

use std::time::{Duration, Instant};

use egui::{Event, MouseWheelUnit, RawInput, TouchPhase, Vec2};

/// Velocity below this (points a second) ends a fling.
const STOP: f32 = 30.;
/// Fraction of the velocity kept after one second of fling.
const FRICTION: f32 = 0.04;
/// Finger events closer together than this belong to one movement.
const GAP: Duration = Duration::from_millis(60);

#[derive(Default)]
pub struct Momentum {
    /// Points a second, smoothed over the last finger events.
    velocity: Vec2,
    last: Option<Instant>,
    fling: Option<(Vec2, Instant)>,
}

impl Momentum {
    /// Scales touchpad deltas and adds the fling's own scroll events.
    pub fn apply(&mut self, input: &mut RawInput, speed: f32) {
        let now = Instant::now();
        let mut ended = false;
        let mut wheel = false;
        for event in &mut input.events {
            match event {
                Event::MouseWheel { unit: MouseWheelUnit::Point, delta, phase, .. } => {
                    *delta *= speed;
                    wheel = true;
                    self.fling = None;
                    match phase {
                        TouchPhase::Start => {
                            self.velocity = Vec2::ZERO;
                            self.last = Some(now);
                        }
                        TouchPhase::Move => {
                            let dt = self.last.map(|t| now - t).filter(|d| *d < GAP);
                            if let Some(dt) = dt {
                                let v = *delta / dt.as_secs_f32().max(0.004);
                                self.velocity = self.velocity * 0.6 + v * 0.4;
                            } else {
                                self.velocity = Vec2::ZERO;
                            }
                            self.last = Some(now);
                        }
                        TouchPhase::End | TouchPhase::Cancel => ended = matches!(phase, TouchPhase::End),
                    }
                }
                Event::MouseWheel { .. } | Event::PointerButton { pressed: true, .. } => self.stop(),
                _ => {}
            }
        }
        // Fingers lifted: fling, unless they rested before lifting.
        let fresh = self.last.is_some_and(|t| now - t < GAP * 2);
        if ended && fresh && self.velocity.length() > STOP * 4. {
            self.fling = Some((self.velocity, now));
        }
        if ended {
            self.velocity = Vec2::ZERO;
            self.last = None;
        }
        if wheel {
            return;
        }
        let Some((velocity, at)) = self.fling.as_mut() else { return };
        let dt = (now - *at).as_secs_f32().min(0.05);
        *at = now;
        *velocity *= FRICTION.powf(dt);
        if velocity.length() < STOP {
            self.fling = None;
            return;
        }
        input.events.push(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: *velocity * dt,
            phase: TouchPhase::Move,
            source: egui::MouseWheelSource::Momentum,
            modifiers: egui::Modifiers::NONE,
        });
    }

    pub fn flinging(&self) -> bool {
        self.fling.is_some()
    }

    fn stop(&mut self) {
        self.fling = None;
        self.velocity = Vec2::ZERO;
        self.last = None;
    }
}
