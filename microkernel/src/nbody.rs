//! Fixed-point gravitational n-body simulation.
//!
//! Used as a heavyweight, branch-and-memory-dense workload whose result is
//! deterministic across architectures (pure integer math). The host runner
//! checks that all three arches report the same `NBODY_CKSUM`.

use crate::fixed::{Fixed, Vec3};

#[derive(Clone, Copy)]
pub struct Body {
    pub pos: Vec3,
    pub vel: Vec3,
    pub mass: Fixed,
}

pub struct NBodySimulation {
    bodies: *mut Body,
    count: usize,
}

impl NBodySimulation {
    pub fn new(bodies: *mut Body, count: usize) -> Self {
        NBodySimulation { bodies, count }
    }

    fn body(&self, i: usize) -> Body {
        unsafe { *self.bodies.add(i) }
    }

    pub fn step(&mut self, dt: Fixed) {
        let g = Fixed::from_int(1);
        let softening = Fixed::from_int(1);

        for i in 0..self.count {
            let mut acc = Vec3::default();
            let bi = self.body(i);
            for j in 0..self.count {
                if i == j {
                    continue;
                }
                let bj = self.body(j);
                let r = bj.pos - bi.pos;
                let dist_sq = r.magnitude_squared() + softening;
                let dist = dist_sq.sqrt();
                let dist_cubed = dist_sq * dist;
                if dist_cubed.0 != 0 {
                    let force_mag = g * bj.mass / dist_cubed;
                    acc += r * force_mag;
                }
            }
            unsafe {
                (*self.bodies.add(i)).vel += acc * dt;
            }
        }

        for i in 0..self.count {
            unsafe {
                let body = &mut *self.bodies.add(i);
                body.pos += body.vel * dt;
            }
        }
    }

    pub fn total_energy(&self) -> Fixed {
        let mut kinetic = Fixed::zero();
        let mut potential = Fixed::zero();
        for i in 0..self.count {
            let bi = self.body(i);
            let v_sq = bi.vel.magnitude_squared();
            kinetic += bi.mass * v_sq / Fixed::from_int(2);
            for j in (i + 1)..self.count {
                let bj = self.body(j);
                let r = (bj.pos - bi.pos).magnitude();
                if r.0 != 0 {
                    potential -= bi.mass * bj.mass / r;
                }
            }
        }
        kinetic + potential
    }

    /// Order-sensitive checksum over every position/velocity word. Deterministic
    /// for a given step count on every architecture.
    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |v: i64| {
            h ^= v as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for i in 0..self.count {
            let b = self.body(i);
            mix(b.pos.x.0);
            mix(b.pos.y.0);
            mix(b.pos.z.0);
            mix(b.vel.x.0);
            mix(b.vel.y.0);
            mix(b.vel.z.0);
        }
        h
    }
}
