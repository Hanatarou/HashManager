//! Headless egui renderer: runs the UI, tessellates, and rasterizes triangles into an RGBA image.
use eframe::egui::{Color32, TextureId};
use std::collections::HashMap;

struct Tex { w: usize, h: usize, px: Vec<Color32> }

pub struct Headless { pub ctx: eframe::egui::Context, texs: HashMap<TextureId, Tex>, time: f64 }

impl Headless {
    pub fn new() -> Self { Headless { ctx: eframe::egui::Context::default(), texs: HashMap::new(), time: 0.0 } }

    fn apply(&mut self, delta: &eframe::egui::TexturesDelta) {
        for (id, ds) in &delta.set {
          for d in ds {
            let eframe::egui::ImageData::Color(img) = &d.image;
            let [w, h] = img.size;
            match d.pos {
                None => { self.texs.insert(*id, Tex { w, h, px: img.pixels.clone() }); }
                Some([x0, y0]) => {
                    if let Some(t) = self.texs.get_mut(id) {
                        for y in 0..h { for x in 0..w { t.px[(y0 + y) * t.w + x0 + x] = img.pixels[y * w + x]; } }
                    }
                }
            }
          }
        }
        for id in &delta.free { self.texs.remove(id); }
    }

    pub fn render(&mut self, w: u32, h: u32, ppp: f32, bg: Color32, mut run: impl FnMut(&mut eframe::egui::Ui)) -> image::RgbaImage {
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let mut raw = eframe::egui::RawInput {
                screen_rect: Some(eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(w as f32, h as f32))),
                ..Default::default()
            };
            raw.viewports.entry(raw.viewport_id).or_default().native_pixels_per_point = Some(ppp);
            self.time += 0.1;
            raw.time = Some(self.time);
            let mut out = self.ctx.run_ui(raw, |ui| run(ui));
            self.apply(&out.textures_delta);
            // Applied above; egui panics in a debug build when a delta is dropped without being cleared.
            out.textures_delta.clear();
            shapes = out.shapes;
        }
        let prims = self.ctx.tessellate(shapes, ppp);
        let (pw, ph) = ((w as f32 * ppp) as usize, (h as f32 * ppp) as usize);
        let mut buf: Vec<[f32; 4]> = vec![[bg.r() as f32 / 255.0, bg.g() as f32 / 255.0, bg.b() as f32 / 255.0, 1.0]; pw * ph];
        for p in prims {
            let eframe::egui::epaint::Primitive::Mesh(m) = p.primitive else { continue };
            let Some(tex) = self.texs.get(&m.texture_id) else { continue };
            let clip = p.clip_rect;
            let (cx0, cy0, cx1, cy1) = ((clip.min.x * ppp).max(0.0), (clip.min.y * ppp).max(0.0), (clip.max.x * ppp).min(pw as f32), (clip.max.y * ppp).min(ph as f32));
            for tri in m.indices.as_chunks::<3>().0.iter() {
                let v = [m.vertices[tri[0] as usize], m.vertices[tri[1] as usize], m.vertices[tri[2] as usize]];
                let pt = |i: usize| (v[i].pos.x * ppp, v[i].pos.y * ppp);
                let (p0, p1, p2) = (pt(0), pt(1), pt(2));
                let minx = p0.0.min(p1.0).min(p2.0).max(cx0).floor() as i32;
                let maxx = p0.0.max(p1.0).max(p2.0).min(cx1).ceil() as i32;
                let miny = p0.1.min(p1.1).min(p2.1).max(cy0).floor() as i32;
                let maxy = p0.1.max(p1.1).max(p2.1).min(cy1).ceil() as i32;
                let den = (p1.1 - p2.1) * (p0.0 - p2.0) + (p2.0 - p1.0) * (p0.1 - p2.1);
                if den.abs() < 1e-9 { continue; }
                for y in miny..maxy {
                    for x in minx..maxx {
                        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                        let a = ((p1.1 - p2.1) * (fx - p2.0) + (p2.0 - p1.0) * (fy - p2.1)) / den;
                        let b = ((p2.1 - p0.1) * (fx - p2.0) + (p0.0 - p2.0) * (fy - p2.1)) / den;
                        let c = 1.0 - a - b;
                        if a < -0.001 || b < -0.001 || c < -0.001 { continue; }
                        let u = a * v[0].uv.x + b * v[1].uv.x + c * v[2].uv.x;
                        let t = a * v[0].uv.y + b * v[1].uv.y + c * v[2].uv.y;
                        let tx = ((u * tex.w as f32) as usize).min(tex.w - 1);
                        let ty = ((t * tex.h as f32) as usize).min(tex.h - 1);
                        let tp = tex.px[ty * tex.w + tx];
                        let col = |ch: usize| -> f32 {
                            let vc = a * v[0].color.to_array()[ch] as f32 + b * v[1].color.to_array()[ch] as f32 + c * v[2].color.to_array()[ch] as f32;
                            vc / 255.0 * tp.to_array()[ch] as f32 / 255.0
                        };
                        let (sr, sg, sb, sa) = (col(0), col(1), col(2), col(3));
                        let d = &mut buf[y as usize * pw + x as usize];
                        d[0] = sr + d[0] * (1.0 - sa);
                        d[1] = sg + d[1] * (1.0 - sa);
                        d[2] = sb + d[2] * (1.0 - sa);
                    }
                }
            }
        }
        let mut img = image::RgbaImage::new(pw as u32, ph as u32);
        for (i, px) in img.pixels_mut().enumerate() {
            let b = buf[i];
            *px = image::Rgba([(b[0].clamp(0.0, 1.0) * 255.0) as u8, (b[1].clamp(0.0, 1.0) * 255.0) as u8, (b[2].clamp(0.0, 1.0) * 255.0) as u8, 255]);
        }
        img
    }
}
