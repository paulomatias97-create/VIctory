//! A picture of ours laid over every presented frame (the phone of
//! `victory-music`). Whoever draws it registers a provider; at each present
//! the provider is asked for a picture of the output's size (premultiplied
//! RGBA8, the compositor blends with `ONE, INV_SRC_ALPHA`), the part that
//! changed since the last one is written to upload memory, and the copy
//! into our texture is recorded at the start of the present's own list
//! ([`record`]), ahead of the composition that draws the texture as the
//! last plane.
//!
//! Nothing here runs without a provider, and a provider that returns
//! `None` (nothing to show) adds no plane.

use std::sync::{Arc, Mutex, OnceLock};

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use crate::host::{Host, transition};

/// A picture to lay over the frame.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// Premultiplied RGBA8, `width * height * 4` bytes, rows top to bottom.
    pub pixels: Vec<u8>,
    /// The part that changed since the picture with the previous serial
    /// (x, y, width, height); the whole picture when unsure.
    pub changed: (u32, u32, u32, u32),
    /// Changes whenever `pixels` do; an unchanged serial is not uploaded.
    pub serial: u64,
}

type Provider = dyn Fn(u32, u32) -> Option<Arc<Picture>> + Send + Sync;

static PROVIDER: OnceLock<Box<Provider>> = OnceLock::new();

/// Registers the provider, asked at each present for a picture of the
/// given width and height. Only the first registration counts.
pub fn set_provider(
    provider: impl Fn(u32, u32) -> Option<Arc<Picture>> + Send + Sync + 'static,
) -> bool {
    PROVIDER.set(Box::new(provider)).is_ok()
}

/// The size the picture is asked for, for an output of `width` by
/// `height`: at most 1080 lines (the compositor stretches it to the
/// output), the output's shape.
pub fn picture_size(width: u32, height: u32) -> (u32, u32) {
    if height <= 1080 || height == 0 {
        return (width, height);
    }
    let w = (u64::from(width) * 1080 / u64::from(height)) as u32;
    (w.max(1), 1080)
}

struct Shown {
    width: u32,
    height: u32,
    texture: ID3D12Resource,
    _upload: ID3D12Resource,
    data: *mut u8,
    footprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
    serial: Option<u64>,
    /// A copy written to upload memory and not recorded yet.
    pending: Option<(u32, u32, u32, u32)>,
    /// The texture is still in its creation state (copy destination).
    fresh: bool,
}

// SAFETY: `data` is the mapping of `_upload`, which lives as long; it is
// only touched under the `SHOWN` lock.
unsafe impl Send for Shown {}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

/// The pixel shader resource state the texture is kept in between frames,
/// as the compositor expects of a plane.
pub(crate) const STATE: u32 = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE.0 as u32;

/// The overlay's texture for an output of `width` by `height`, its new
/// pixels written to upload memory for [`record`]; `None` with nothing to
/// show.
pub(crate) fn prepare(host: &Host, width: u32, height: u32) -> Option<ID3D12Resource> {
    let provider = PROVIDER.get()?;
    let (w, h) = picture_size(width, height);
    let picture = provider(w, h)?;
    if picture.width == 0
        || picture.height == 0
        || picture.pixels.len() < (picture.width * picture.height * 4) as usize
    {
        return None;
    }
    let mut shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    if shown
        .as_ref()
        .is_none_or(|s| (s.width, s.height) != (picture.width, picture.height))
    {
        *shown = Some(make(host, picture.width, picture.height)?);
    }
    let s = shown.as_mut()?;
    if s.serial != Some(picture.serial) {
        // A new texture takes the whole picture.
        let (x, y, cw, ch) = if s.serial.is_none() {
            (0, 0, s.width, s.height)
        } else {
            clamp(picture.changed, s.width, s.height)
        };
        let pitch = s.footprint.Footprint.RowPitch as usize;
        for row in y..y + ch {
            let from = ((row * s.width + x) * 4) as usize;
            let len = (cw * 4) as usize;
            // SAFETY: inside the mapped upload buffer (the footprint covers
            // every row at `pitch`) and inside `pixels` (checked above).
            unsafe {
                std::ptr::copy_nonoverlapping(
                    picture.pixels.as_ptr().add(from),
                    s.data
                        .add(s.footprint.Offset as usize + row as usize * pitch + x as usize * 4),
                    len,
                );
            }
        }
        // Grow a pending copy rather than lose it (a present that did not
        // record it, say).
        s.pending = Some(match s.pending {
            Some(p) => union(p, (x, y, cw, ch)),
            None => (x, y, cw, ch),
        });
        s.serial = Some(picture.serial);
    }
    Some(s.texture.clone())
}

/// Records the pending copy into the overlay's texture on `list`, leaving
/// it in [`STATE`]. Nothing when there is none.
pub(crate) fn record(list: &ID3D12GraphicsCommandList) {
    let mut shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    let Some(s) = shown.as_mut() else {
        return;
    };
    let Some((x, y, w, h)) = s.pending.take() else {
        return;
    };
    if w == 0 || h == 0 {
        return;
    }
    // SAFETY: our texture and upload buffer, alive while `SHOWN` holds
    // them; the list is open.
    unsafe {
        if !s.fresh {
            list.ResourceBarrier(&[transition(
                &s.texture,
                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                D3D12_RESOURCE_STATE_COPY_DEST,
            )]);
        }
        let dst = D3D12_TEXTURE_COPY_LOCATION {
            pResource: std::mem::transmute_copy(&s.texture),
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                SubresourceIndex: 0,
            },
        };
        let src = D3D12_TEXTURE_COPY_LOCATION {
            pResource: std::mem::transmute_copy(&s._upload),
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                PlacedFootprint: s.footprint,
            },
        };
        let region = D3D12_BOX {
            left: x,
            top: y,
            front: 0,
            right: x + w,
            bottom: y + h,
            back: 1,
        };
        list.CopyTextureRegion(&dst, x, y, 0, &src, Some(&region));
        list.ResourceBarrier(&[transition(
            &s.texture,
            D3D12_RESOURCE_STATE_COPY_DEST,
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
        )]);
    }
    s.fresh = false;
}

/// A texture of `width` by `height` and upload memory for all of it.
fn make(host: &Host, width: u32, height: u32) -> Option<Shown> {
    let desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        Width: u64::from(width),
        Height: height,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        ..Default::default()
    };
    let heap = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_DEFAULT,
        ..Default::default()
    };
    let mut texture: Option<ID3D12Resource> = None;
    let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
    let mut total = 0u64;
    // SAFETY: plain creation; outputs for one subresource.
    let made = unsafe {
        host.device.GetCopyableFootprints(
            &desc,
            0,
            1,
            0,
            Some(&mut footprint),
            None,
            None,
            Some(&mut total),
        );
        host.device.CreateCommittedResource(
            &heap,
            D3D12_HEAP_FLAG_NONE,
            &desc,
            D3D12_RESOURCE_STATE_COPY_DEST,
            None,
            &mut texture,
        )
    };
    if let Err(e) = made {
        tracing::warn!(error = %e, width, height, "no overlay texture");
        return None;
    }
    let (upload, data) = host.upload_buffer(total)?;
    // Start transparent: rows the provider never sends stay clear.
    // SAFETY: the mapping covers `total` bytes.
    unsafe { std::ptr::write_bytes(data, 0, total as usize) };
    Some(Shown {
        width,
        height,
        texture: texture?,
        _upload: upload,
        data,
        footprint,
        serial: None,
        pending: None,
        fresh: true,
    })
}

/// `r` cut to a `width` by `height` picture.
fn clamp((x, y, w, h): (u32, u32, u32, u32), width: u32, height: u32) -> (u32, u32, u32, u32) {
    let x = x.min(width);
    let y = y.min(height);
    (x, y, w.min(width - x), h.min(height - y))
}

fn union(a: (u32, u32, u32, u32), b: (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
    let x = a.0.min(b.0);
    let y = a.1.min(b.1);
    let r = (a.0 + a.2).max(b.0 + b.2);
    let bt = (a.1 + a.3).max(b.1 + b.3);
    (x, y, r - x, bt - y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_are_at_most_1080_lines_in_the_output_shape() {
        assert_eq!(picture_size(1920, 1080), (1920, 1080));
        assert_eq!(picture_size(1280, 720), (1280, 720));
        assert_eq!(picture_size(3840, 2160), (1920, 1080));
        assert_eq!(picture_size(2560, 1440), (1920, 1080));
        assert_eq!(picture_size(3440, 1440), (2580, 1080));
    }

    #[test]
    fn regions_clamp_and_grow() {
        assert_eq!(clamp((1900, 0, 100, 2000), 1920, 1080), (1900, 0, 20, 1080));
        assert_eq!(union((10, 10, 5, 5), (0, 12, 2, 10)), (0, 10, 15, 12));
    }
}
