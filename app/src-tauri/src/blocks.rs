use std::collections::HashMap;

use image::RgbaImage;
use serde::Serialize;

use crate::ocr::OcrLine;

pub struct Block {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub text: String,
    pub lines: u32,
    line_h: f32,
    bottom: f32,
    left: f32,
    center: f32,
}

#[derive(Serialize, Clone)]
pub struct BlockOut {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub lines: u32,
    pub text: String,
    pub bg: String,
    pub fg: String,
}

pub fn group(mut lines: Vec<OcrLine>) -> Vec<Block> {
    lines.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut blocks: Vec<Block> = Vec::new();
    for l in lines {
        let target = blocks.iter_mut().rev().find(|b| {
            let gap = l.y - b.bottom;
            let ratio = l.h / b.line_h.max(1.0);
            let near_left = (l.x - b.left).abs() < b.line_h * 1.5;
            let near_center = ((l.x + l.w / 2.0) - b.center).abs() < b.line_h * 1.5;
            gap > -b.line_h * 0.3
                && gap < b.line_h * 0.8
                && ratio > 0.7
                && ratio < 1.4
                && (near_left || near_center)
        });

        match target {
            Some(b) => {
                let right = (b.x + b.w).max(l.x + l.w);
                let bottom = (b.y + b.h).max(l.y + l.h);
                b.x = b.x.min(l.x);
                b.y = b.y.min(l.y);
                b.w = right - b.x;
                b.h = bottom - b.y;
                b.text.push('\n');
                b.text.push_str(&l.text);
                b.lines += 1;
                b.line_h = l.h;
                b.bottom = l.y + l.h;
                b.left = l.x;
                b.center = l.x + l.w / 2.0;
            }
            None => blocks.push(Block {
                x: l.x,
                y: l.y,
                w: l.w,
                h: l.h,
                text: l.text,
                lines: 1,
                line_h: l.h,
                bottom: l.y + l.h,
                left: l.x,
                center: l.x + l.w / 2.0,
            }),
        }
    }
    blocks
}

pub fn translatable(text: &str) -> bool {
    text.chars().filter(|c| c.is_alphabetic()).count() >= 2
}

fn dist(a: [u8; 3], b: [u8; 3]) -> i32 {
    (0..3).map(|i| (a[i] as i32 - b[i] as i32).abs()).sum()
}

fn luma(c: [u8; 3]) -> f32 {
    0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32
}

fn css(c: [u8; 3]) -> String {
    format!("rgb({},{},{})", c[0], c[1], c[2])
}

pub fn colors(img: &RgbaImage, x: f32, y: f32, w: f32, h: f32) -> (String, String) {
    let x0 = (x.max(0.0) as u32).min(img.width().saturating_sub(1));
    let y0 = (y.max(0.0) as u32).min(img.height().saturating_sub(1));
    let x1 = ((x + w).ceil().max(0.0) as u32).min(img.width());
    let y1 = ((y + h).ceil().max(0.0) as u32).min(img.height());

    let mut bins: HashMap<u16, (u32, [u32; 3])> = HashMap::new();
    for py in y0..y1 {
        for px in x0..x1 {
            let p = img.get_pixel(px, py).0;
            let key = ((p[0] >> 4) as u16) << 8 | ((p[1] >> 4) as u16) << 4 | (p[2] >> 4) as u16;
            let e = bins.entry(key).or_insert((0, [0; 3]));
            e.0 += 1;
            for i in 0..3 {
                e.1[i] += p[i] as u32;
            }
        }
    }
    let mean = |e: &(u32, [u32; 3])| -> [u8; 3] {
        [
            (e.1[0] / e.0) as u8,
            (e.1[1] / e.0) as u8,
            (e.1[2] / e.0) as u8,
        ]
    };

    let Some(bg_bin) = bins.values().max_by_key(|e| e.0) else {
        return ("rgb(20,20,28)".into(), "rgb(255,255,255)".into());
    };
    let bg = mean(bg_bin);

    let fg = bins
        .values()
        .map(|e| (e.0, mean(e)))
        .filter(|(_, c)| dist(*c, bg) > 180)
        .max_by_key(|(n, _)| *n)
        .map(|(_, c)| c)
        .filter(|c| (luma(*c) - luma(bg)).abs() > 90.0)
        .unwrap_or(if luma(bg) < 128.0 { [255, 255, 255] } else { [20, 20, 20] });

    (css(bg), css(fg))
}
