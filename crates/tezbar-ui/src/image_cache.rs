//! A cache owns decoded bytes and every uploaded texture. Call `release` on view hide/drop.
use gpui::{App, RenderImage, Window};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
pub const MAX_ENTRIES: usize = 128;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_DECODE_TASKS: usize = 4;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ticket {
    pub key: String,
    generation: u64,
    serial: u64,
}
struct Entry<T> {
    value: T,
    bytes: usize,
    used: u64,
}
pub struct OwnershipCache<T> {
    entries: HashMap<String, Entry<T>>,
    pending: HashSet<Ticket>,
    generation: u64,
    clock: u64,
    bytes: usize,
    limit: usize,
}
impl<T> Default for OwnershipCache<T> {
    fn default() -> Self {
        Self::new(MAX_BYTES)
    }
}
impl<T> OwnershipCache<T> {
    pub fn new(byte_limit: usize) -> Self {
        Self {
            entries: HashMap::new(),
            pending: HashSet::new(),
            generation: 0,
            clock: 0,
            bytes: 0,
            limit: byte_limit,
        }
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn request(&mut self, key: &str) -> Option<Ticket> {
        if self.entries.contains_key(key)
            || self.pending.iter().any(|t| t.key == key)
            || self.pending.len() >= MAX_DECODE_TASKS
        {
            return None;
        }
        self.clock += 1;
        let t = Ticket {
            key: key.into(),
            generation: self.generation,
            serial: self.clock,
        };
        self.pending.insert(t.clone());
        Some(t)
    }
    pub fn get(&mut self, key: &str) -> Option<&T> {
        self.clock += 1;
        self.entries.get_mut(key).map(|e| {
            e.used = self.clock;
            &e.value
        })
    }
    pub fn finish(
        &mut self,
        t: Ticket,
        result: Option<(T, usize)>,
        mut release: impl FnMut(T),
    ) -> bool {
        let valid = self.pending.remove(&t) && t.generation == self.generation;
        let Some((value, bytes)) = result else {
            return false;
        };
        if !valid || bytes > self.limit {
            release(value);
            return false;
        }
        if let Some(e) = self.entries.remove(&t.key) {
            self.bytes -= e.bytes;
            release(e.value);
        }
        while self.entries.len() >= MAX_ENTRIES || self.bytes + bytes > self.limit {
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            let e = self.entries.remove(&key).unwrap();
            self.bytes -= e.bytes;
            release(e.value);
        }
        self.clock += 1;
        self.bytes += bytes;
        self.entries.insert(
            t.key,
            Entry {
                value,
                bytes,
                used: self.clock,
            },
        );
        true
    }
    pub fn remove(&mut self, key: &str, mut release: impl FnMut(T)) {
        self.pending.retain(|t| t.key != key);
        if let Some(e) = self.entries.remove(key) {
            self.bytes -= e.bytes;
            release(e.value);
        }
    }
    pub fn release(&mut self, mut release: impl FnMut(T)) {
        self.generation = self.generation.wrapping_add(1);
        self.pending.clear();
        for (_, e) in self.entries.drain() {
            release(e.value);
        }
        self.bytes = 0;
    }
}
/// Reject excessive compressed and decoded allocations before uploading. No icon is decoded at startup.
pub fn decode_png(bytes: &[u8]) -> Result<image::RgbaImage, String> {
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Image exceeds 2 MiB".into());
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let mut rgba = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
    rgba_to_bgra(rgba.as_mut());
    Ok(rgba)
}
pub fn rgba_to_bgra(bytes: &mut [u8]) {
    for pixel in bytes.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}
pub struct ImageCache(OwnershipCache<Arc<RenderImage>>);
impl Default for ImageCache {
    fn default() -> Self {
        Self(OwnershipCache::default())
    }
}
impl ImageCache {
    pub fn request(&mut self, key: &str) -> Option<Ticket> {
        self.0.request(key)
    }
    pub fn get(&mut self, key: &str) -> Option<Arc<RenderImage>> {
        self.0.get(key).cloned()
    }
    pub fn complete(
        &mut self,
        ticket: Ticket,
        decoded: Option<image::RgbaImage>,
        w: &mut Window,
        cx: &mut App,
    ) -> bool {
        let result = decoded.map(|image| {
            let bytes = image.as_raw().len();
            let frame = image::Frame::new(image);
            (Arc::new(RenderImage::new(vec![frame])), bytes)
        });
        self.0
            .finish(ticket, result, |image| cx.drop_image(image, Some(w)))
    }
    pub fn remove(&mut self, key: &str, w: &mut Window, cx: &mut App) {
        self.0.remove(key, |image| cx.drop_image(image, Some(w)));
    }
    pub fn release(&mut self, w: &mut Window, cx: &mut App) {
        self.0.release(|image| cx.drop_image(image, Some(w)));
    }
    pub fn release_all_windows(&mut self, cx: &mut App) {
        self.0.release(|image| cx.drop_image(image, None));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn bgra_preserves_alpha() {
        let mut data = [1, 2, 3, 4, 10, 20, 30, 40];
        rgba_to_bgra(&mut data);
        assert_eq!(data, [3, 2, 1, 4, 30, 20, 10, 40]);
    }
    #[::core::prelude::v1::test]
    fn stale_and_cancelled_decodes_are_released() {
        let mut c = OwnershipCache::new(20);
        let t = c.request("a").unwrap();
        let mut dropped = vec![];
        c.release(|v: i32| dropped.push(v));
        assert!(!c.finish(t, Some((1, 4)), |v| dropped.push(v)));
        assert_eq!(dropped, [1]);
        assert_eq!(c.bytes(), 0);
    }
    #[::core::prelude::v1::test]
    fn bounded_decoders_and_duplicate_keys() {
        let mut c = OwnershipCache::<u8>::default();
        for n in 0..4 {
            assert!(c.request(&n.to_string()).is_some());
        }
        assert!(c.request("4").is_none());
        assert!(c.request("0").is_none());
    }
    #[::core::prelude::v1::test]
    fn count_byte_lru_and_release_ownership() {
        let mut c = OwnershipCache::new(8);
        let mut released = vec![];
        for n in 0..3 {
            let t = c.request(&n.to_string()).unwrap();
            c.finish(t, Some((n, 4)), |v| released.push(v));
        }
        assert_eq!(released, [0]);
        assert_eq!(c.bytes(), 8);
        c.get("1");
        let t = c.request("3").unwrap();
        c.finish(t, Some((3, 4)), |v| released.push(v));
        assert_eq!(released, [0, 2]);
        c.release(|v| released.push(v));
        released.sort();
        assert_eq!(released, [0, 1, 2, 3]);
        assert_eq!(c.bytes(), 0);
    }
    #[::core::prelude::v1::test]
    fn count_limited_even_for_tiny_images() {
        let mut c = OwnershipCache::default();
        let mut count = 0;
        for n in 0..140 {
            let t = c.request(&n.to_string()).unwrap();
            c.finish(t, Some((n, 1)), |_| count += 1);
        }
        assert_eq!(c.len(), 128);
        assert_eq!(count, 12);
    }
    #[::core::prelude::v1::test]
    fn oversized_image_cannot_enter_cache() {
        let mut c = OwnershipCache::new(5);
        let t = c.request("x").unwrap();
        let mut dropped = false;
        assert!(!c.finish(t, Some((1, 6)), |_| dropped = true));
        assert!(dropped);
    }
    #[::core::prelude::v1::test]
    fn png_decode_enforces_dimensions_and_bgra_contract() {
        use image::ImageEncoder;
        let mut png = vec![];
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[255, 0, 0, 127], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        assert_eq!(decode_png(&png).unwrap().into_raw(), [0, 0, 255, 127]);
        let image = image::RgbaImage::new(2049, 1);
        let mut png = vec![];
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(image.as_raw(), 2049, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        assert!(decode_png(&png).is_err());
    }
}
