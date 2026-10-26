// Copyright Iced staff [Iced](https://github.com/iced-rs/iced/tree/master)
// SPDX-License-Identifier: MIT
// [MIT](http://opensource.org/licenses/MIT)
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

//! Display images in your user interface.
use cosmic::iced_runtime::core::widget::Id;

//use cosmic::iced_core::image;
use cosmic::iced_core::layout;
use cosmic::iced_core::mouse;
use cosmic::iced_core::renderer;
use cosmic::iced_core::widget::Tree;
use cosmic::iced_core::{ContentFit, Element, Layout, Length, Rectangle, Size, Vector, Widget};
use imagesize;

use std::hash::Hash;

pub use cosmic::iced_core::image::{FilterMethod, Handle};

pub use super::image_player::Viewer;

// Creates a new [`Viewer`] with the given image `Handle`.
pub fn _viewer<Handle>(handle: Handle) -> Viewer<Handle> {
    Viewer::new(handle)
}

pub fn create_handle(pathstring: String) -> Handle {
    let mut path = std::path::PathBuf::from(&pathstring);
    match imagesize::size(path.clone()) {
        Ok(dim) => {
            if dim.width > 2000 || dim.height > 2000 {
                // downsize image to be able to display
                path = crate::thumbnails::downscale_image(&path, 2000, dim.width, dim.height);
            }
        }
        Err(why) => println!("Error getting size: {:?}", why),
    }
    let hand = Handle::from_path(&path);
    hand
}

/// A frame that displays an image while keeping aspect ratio.
///
/// # Example
///
/// ```no_run
/// # use iced_widget::image::{self, Image};
/// #
/// let image = Image::<image::Handle>::new("resources/ferris.png");
/// ```
///
/// <img src="https://github.com/iced-rs/iced/blob/9712b319bb7a32848001b96bd84977430f14b623/examples/resources/ferris.png?raw=true" width="300">
#[derive(Debug)]
pub struct Image<'a, Handle> {
    id: Id,
    handle: Handle,
    width: Length,
    height: Length,
    content_fit: ContentFit,
    filter_method: FilterMethod,
    border_radius: [f32; 4],
    phantom_data: std::marker::PhantomData<&'a ()>,
}

impl<'a, Handle> Image<'a, Handle> {
    /// Creates a new [`Image`] with the given path.
    pub fn new<T: Into<Handle>>(handle: T) -> Self {
        Image {
            id: Id::unique(),
            handle: handle.into(),
            width: Length::Shrink,
            height: Length::Shrink,
            content_fit: ContentFit::Contain,
            filter_method: FilterMethod::default(),
            border_radius: [0.0; 4],
            phantom_data: std::marker::PhantomData,
        }
    }

    /// Sets the border radius of the image.
    pub fn border_radius(mut self, border_radius: [f32; 4]) -> Self {
        self.border_radius = border_radius;
        self
    }

    /// Sets the width of the [`Image`] boundaries.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`Image`] boundaries.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the [`ContentFit`] of the [`Image`].
    ///
    /// Defaults to [`ContentFit::Contain`]
    pub fn content_fit(mut self, content_fit: ContentFit) -> Self {
        self.content_fit = content_fit;
        self
    }

    /// Sets the [`FilterMethod`] of the [`Image`].
    pub fn filter_method(mut self, filter_method: FilterMethod) -> Self {
        self.filter_method = filter_method;
        self
    }
}

/// Computes the layout of an [`Image`].
pub fn layout<Renderer, Handle>(
    renderer: &Renderer,
    limits: &layout::Limits,
    handle: &Handle,
    width: Length,
    height: Length,
    content_fit: ContentFit,
    _border_radius: [f32; 4],
) -> layout::Node
where
    Renderer: cosmic::iced_core::image::Renderer<Handle = Handle>,
{
    // The raw w/h of the underlying image
    let image_size = {
        let Size { width, height } = renderer.measure_image(handle);

        Size::new(width as f32, height as f32)
    };

    // The size to be available to the widget prior to `Shrink`ing
    let raw_size = limits.resolve(width, height, image_size);

    // The uncropped size of the image when fit to the bounds above
    let full_size = content_fit.fit(image_size, raw_size);

    // Shrink the widget to fit the resized image, if requested
    let final_size = Size {
        width: match width {
            Length::Shrink => f32::min(raw_size.width, full_size.width),
            _ => raw_size.width,
        },
        height: match height {
            Length::Shrink => f32::min(raw_size.height, full_size.height),
            _ => raw_size.height,
        },
    };

    layout::Node::new(final_size)
}

/// Draws an [`Image`]
pub fn draw<Renderer, Handle>(
    renderer: &mut Renderer,
    layout: Layout<'_>,
    handle: &Handle,
    content_fit: ContentFit,
    filter_method: FilterMethod,
    border_radius: [f32; 4],
) where
    Renderer: cosmic::iced_core::image::Renderer<Handle = Handle>,
    Handle: Clone + Hash,
{
    let Size { width, height } = renderer.measure_image(handle);
    let image_size = Size::new(width as f32, height as f32);

    let bounds = layout.bounds();
    let adjusted_fit = content_fit.fit(image_size, bounds.size());

    let render = |renderer: &mut Renderer| {
        let offset = Vector::new(
            (bounds.width - adjusted_fit.width).max(0.0) / 2.0,
            (bounds.height - adjusted_fit.height).max(0.0) / 2.0,
        );

        let drawing_bounds = Rectangle {
            width: adjusted_fit.width,
            height: adjusted_fit.height,
            ..bounds
        };

        renderer.draw_image(
            handle.clone(),
            filter_method,
            drawing_bounds + offset,
            cosmic::iced::Radians::from(0.0),
            1.0,
            border_radius,
        );
    };

    if adjusted_fit.width > bounds.width || adjusted_fit.height > bounds.height {
        renderer.with_layer(bounds, render);
    } else {
        render(renderer);
    }
}

impl<'a, Message, Theme, Renderer, Handle> Widget<Message, Theme, Renderer> for Image<'a, Handle>
where
    Renderer: cosmic::iced_core::image::Renderer<Handle = Handle>,
    Handle: Clone + Hash,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &self,
        _tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout(
            renderer,
            limits,
            &self.handle,
            self.width,
            self.height,
            self.content_fit,
            self.border_radius,
        )
    }

    fn draw(
        &self,
        _state: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        draw(
            renderer,
            layout,
            &self.handle,
            self.content_fit,
            self.filter_method,
            self.border_radius,
        );
    }

    fn id(&self) -> Option<Id> {
        Some(self.id.clone())
    }

    fn set_id(&mut self, id: Id) {
        self.id = id;
    }
}

impl<'a, Message, Theme, Renderer, Handle> From<Image<'a, Handle>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: cosmic::iced_core::image::Renderer<Handle = Handle>,
    Handle: Clone + Hash + 'a,
{
    fn from(image: Image<'a, Handle>) -> Element<'a, Message, Theme, Renderer> {
        Element::new(image)
    }
}
