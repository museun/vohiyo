use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use egui::{TextureHandle, TextureOptions, Vec2};

pub enum Image {
    Static(TextureHandle),
    Animated(Animated),
}

impl Image {
    pub fn as_egui_image(&self, size: Vec2, dt: f32) -> egui::Image {
        match self {
            Self::Static(image) => egui::Image::new(image, size),
            Self::Animated(animated) => animated.get_frame(dt, size),
        }
    }

    pub fn load_rgba_data(ctx: &egui::Context, name: &str, data: &[u8]) -> anyhow::Result<Self> {
        const GUESS_SIZE: usize = 64;
        anyhow::ensure!(
            data.len() >= GUESS_SIZE,
            "incomplete image for '{name}' ({bytes} {bytes}). ignoring",
            bytes = data.len()
        );

        match ::image::guess_format(&data[..data.len().min(GUESS_SIZE)])
            .map_err(|err| anyhow::anyhow!("cannot guess format for '{name}': {err}"))?
        {
            ::image::ImageFormat::Png => {
                let dec = ::image::codecs::png::PngDecoder::new(data).map_err(|err| {
                    anyhow::anyhow!("expected png, got something else for '{name}': {err}")
                })?;

                if dec.is_apng() {
                    Self::load_apng(ctx, name, data)
                } else {
                    Self::load_texture_handle(ctx, name, data)
                }
            }
            ::image::ImageFormat::Jpeg => Self::load_texture_handle(ctx, name, data),
            ::image::ImageFormat::Gif => Self::load_gif(ctx, name, data),
            fmt => {
                anyhow::bail!("unsupported format for '{name}': {fmt:?}")
            }
        }
    }

    fn load_texture_handle(ctx: &egui::Context, name: &str, data: &[u8]) -> anyhow::Result<Self> {
        let img = ::image::load_from_memory(data)?;
        let data = img.to_rgba8();
        let (width, height) = data.dimensions();
        let image = egui::ColorImage::from_rgba_unmultiplied([width as _, height as _], &data);
        let handle = ctx.load_texture(name, image, TextureOptions::default());
        Ok(Self::Static(handle))
    }

    fn load_apng(ctx: &egui::Context, name: &str, data: &[u8]) -> anyhow::Result<Self> {
        let dec = ::image::codecs::png::PngDecoder::new(data)?;
        anyhow::ensure!(dec.is_apng(), "expected an animated png");
        Animated::load_frames(ctx, name, dec.apng()).map(Self::Animated)
    }

    fn load_gif(ctx: &egui::Context, name: &str, data: &[u8]) -> anyhow::Result<Self> {
        let dec = ::image::codecs::gif::GifDecoder::new(data)?;
        Animated::load_frames(ctx, name, dec).map(Self::Animated)
    }
}

pub struct Animated {
    // TODO use an f32 here so we can compenstate for render lag
    frames: Vec<(Duration, TextureHandle)>,
    // TODO use an f32 here
    last: Cell<Option<Instant>>,
    pos: Cell<usize>,
}

impl Animated {
    fn get_frame(&self, dt: f32, size: Vec2) -> egui::Image {
        let pos = self.pos.get();

        let (delay, frame) = &self.frames[pos];
        match self.last.get() {
            Some(last) if last.elapsed().as_secs_f32() >= delay.as_secs_f32() - dt => {
                self.pos.set((pos + 1) % self.frames.len());
                self.last.set(Some(Instant::now()))
            }
            Some(..) => {}
            None => {
                self.last.set(Some(Instant::now()));
            }
        }

        egui::Image::new(frame, size)
    }

    fn load_frames<'a>(
        ctx: &egui::Context,
        name: &str,
        decoder: impl ::image::AnimationDecoder<'a>,
    ) -> anyhow::Result<Self> {
        decoder
            .into_frames()
            .map(|frame| {
                let frame = frame?;
                let delay = Duration::from(frame.delay());
                let data = frame.into_buffer();
                let (width, height) = data.dimensions();
                let image =
                    egui::ColorImage::from_rgba_unmultiplied([width as _, height as _], &data);
                let handle = ctx.load_texture(name, image, TextureOptions::default());
                Ok((delay, handle))
            })
            .collect::<anyhow::Result<_>>()
            .map(|frames| Self {
                frames,
                last: Cell::default(),
                pos: Cell::default(),
            })
    }
}
