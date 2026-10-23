//! Engine playground. Phase 0: one breathing circle with orbiting satellites,
//! proving the pipeline natively, in the browser and headless.
//!
//! Native: `cargo run -p sandbox`, or `cargo run -p sandbox -- --snapshot out.png`
//! for a headless strip of frames.

use engine::glam::Vec2;
use engine::{App, Frame, Shapes, rgb, rgba};

struct Sandbox;

impl App for Sandbox {
    fn frame(&mut self, f: &Frame, s: &mut Shapes) {
        s.clear = rgb(0x14121a);
        let center = f.size * 0.5;
        let unit = f.size.min_element();

        let breath = 1.0 + 0.06 * (f.time * 2.0).sin();
        let radius = unit * 0.16 * breath;
        s.soft_circle(center, radius * 1.15, radius * 0.6, rgba(0xe8c547, 0.25));
        s.circle(center, radius, rgb(0xe8c547));

        let palette = [0x6fb3d2, 0xd96c75, 0x8fcf7a];
        for (i, hex) in palette.into_iter().enumerate() {
            let angle = f.time * 0.9 + i as f32 * std::f32::consts::TAU / 3.0;
            let orbit = center + Vec2::from_angle(angle) * unit * 0.32;
            s.circle(orbit, unit * 0.05, rgb(hex));
        }
    }
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let args: Vec<String> = std::env::args().collect();
        if let Some(i) = args.iter().position(|a| a == "--snapshot") {
            let path = args.get(i + 1).map_or("snapshot.png", String::as_str);
            engine::headless::snapshot_strip(&mut Sandbox, 320, 320, &[0.0, 0.8, 1.6, 2.4], path.as_ref());
            println!("wrote {path}");
            return;
        }
    }
    engine::run(Sandbox);
}
