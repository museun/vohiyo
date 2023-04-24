use crate::{image::Image, resolver};

use super::ImageFetcher;

pub struct ImageCache {
    images: resolver::ResolverMap<String, Image, (String, Option<Image>)>,
    fetcher: ImageFetcher,
}

impl ImageCache {
    pub fn new(http: reqwest::Client, ctx: egui::Context) -> Self {
        Self {
            images: resolver::ResolverMap::new(),
            fetcher: ImageFetcher::new(http, ctx),
        }
    }

    pub fn set(&mut self, url: String, image: Image) {
        self.images.update().set(url, image);
    }

    pub fn get_image(&mut self, url: &str) -> Option<&Image> {
        self.images
            .get_or_update(url, |url| self.fetcher.get_image(url))
    }

    pub fn poll(&mut self) {
        self.images.poll(|entry, (k, v)| match v {
            Some(v) => {
                eprintln!("fetched image: {k}");
                entry.set(k, v);
            }
            None => {
                eprintln!("could not fetch image: {k}")
            }
        });
    }
}
