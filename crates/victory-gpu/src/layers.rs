//! The planes of a present drawn one over the other, as the console's
//! display composes them (after PastiiJay/klbf-lab 67f8a49f): in Series S
//! mode the title presents its world at 1280x720 in one plane and its
//! interface (HUD, subtitles, map) at full size in another, and showing
//! only the first left the interface out. Each layer's source rectangle is
//! drawn bilinearly over its place in the window, the first as it is, the
//! others blended over it with premultiplied alpha
//! (`VICTORY_PRESENT_STRAIGHT=1`: straight alpha).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::RECT;
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use crate::host::{Host, compile, host, transition};

/// One resource of a present plane.
#[derive(Clone)]
pub(crate) struct Layer {
    pub(crate) texture: ID3D12Resource,
    /// Its host state, which it stays in.
    pub(crate) state: u32,
    /// The texels shown (left, top, right, bottom); `None`: all of them.
    pub(crate) source: Option<[i32; 4]>,
    /// Where they go, in fractions of the window; `None`: all of it.
    pub(crate) place: Option<[f32; 4]>,
}

impl Layer {
    pub(crate) fn texels(&self, width: u32, height: u32) -> [i32; 4] {
        texels(self.source, width, height)
    }

    pub(crate) fn fractions(&self) -> [f32; 4] {
        fractions(self.place)
    }
}

fn texels(source: Option<[i32; 4]>, width: u32, height: u32) -> [i32; 4] {
    let (w, h) = (width as i32, height as i32);
    match source {
        Some([l, t, r, b]) => {
            let [l, r] = [l.clamp(0, w), r.clamp(0, w)];
            let [t, b] = [t.clamp(0, h), b.clamp(0, h)];
            if r > l && b > t { [l, t, r, b] } else { [0, 0, w, h] }
        }
        None => [0, 0, w, h],
    }
}

/// The whole window if no placement, or one that is not a rectangle of
/// fractions, is given.
fn fractions(place: Option<[f32; 4]>) -> [f32; 4] {
    match place {
        Some([l, t, r, b]) if [l, t, r, b].iter().all(|v| (-0.01..=1.01).contains(v)) && r > l && b > t => [l, t, r, b],
        _ => [0.0, 0.0, 1.0, 1.0],
    }
}

const LAYER: &str = r#"
#define RS "RootConstants(num32BitConstants = 4, b0), DescriptorTable(SRV(t0), visibility = SHADER_VISIBILITY_PIXEL), StaticSampler(s0, filter = FILTER_MIN_MAG_MIP_LINEAR, addressU = TEXTURE_ADDRESS_CLAMP, addressV = TEXTURE_ADDRESS_CLAMP, addressW = TEXTURE_ADDRESS_CLAMP)"
cbuffer source : register(b0) { float4 source; };
Texture2DArray<float4> plane : register(t0);
SamplerState bilinear : register(s0);
struct Corner { float4 p : SV_Position; float2 uv : TEXCOORD0; };
[RootSignature(RS)]
Corner corner(uint i : SV_VertexID) {
    float2 t = float2(i & 1, i >> 1) * 2.0;
    Corner c;
    c.p = float4(t.x * 2.0 - 1.0, 1.0 - t.y * 2.0, 0.0, 1.0);
    c.uv = lerp(source.xy, source.zw, t);
    return c;
}
[RootSignature(RS)]
float4 layer(Corner c) : SV_Target {
    return plane.Sample(bilinear, float3(c.uv, 0.0));
}
"#;

const VIEWS: u32 = 256;

type Pipeline = (ID3D12RootSignature, ID3D12PipelineState);

struct Pass {
    views: ID3D12DescriptorHeap,
    cpu: usize,
    gpu: u64,
    step: u32,
    next: AtomicU32,
    made: Mutex<Vec<(bool, i32, Option<Pipeline>)>>,
}

// SAFETY: D3D12 objects are free-threaded.
unsafe impl Send for Pass {}
// SAFETY: as above.
unsafe impl Sync for Pass {}

fn pass() -> Option<&'static Pass> {
    static PASS: OnceLock<Option<Pass>> = OnceLock::new();
    PASS.get_or_init(|| {
        let host = host()?;
        let kind = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
        // SAFETY: plain heap creation and queries.
        unsafe {
            let views: ID3D12DescriptorHeap = host
                .device
                .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: kind,
                    NumDescriptors: VIEWS,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                    NodeMask: 0,
                })
                .ok()?;
            Some(Pass {
                cpu: views.GetCPUDescriptorHandleForHeapStart().ptr,
                gpu: views.GetGPUDescriptorHandleForHeapStart().ptr,
                step: host.device.GetDescriptorHandleIncrementSize(kind),
                views,
                next: AtomicU32::new(0),
                made: Mutex::default(),
            })
        }
    })
    .as_ref()
}

impl Pass {
    fn pipeline(&self, blended: bool, format: DXGI_FORMAT) -> Option<Pipeline> {
        let mut made = self.made.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, _, p)) = made.iter().find(|(b, f, _)| *b == blended && *f == format.0) {
            return p.clone();
        }
        let p = match make(blended, format) {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!(error = %e, "no pipeline to draw present planes: the first plane is copied alone");
                None
            }
        };
        made.push((blended, format.0, p.clone()));
        p
    }
}

fn straight_alpha() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_PRESENT_STRAIGHT").is_some_and(|v| v == "1"))
}

fn make(blended: bool, format: DXGI_FORMAT) -> windows::core::Result<Pipeline> {
    use windows::core::s;
    let device = &host().ok_or_else(windows::core::Error::empty)?.device;
    let vs = compile(LAYER, s!("corner"), s!("vs_5_1"))?;
    let ps = compile(LAYER, s!("layer"), s!("ps_5_1"))?;
    let code = |bytes: &[u8]| D3D12_SHADER_BYTECODE {
        pShaderBytecode: bytes.as_ptr().cast(),
        BytecodeLength: bytes.len(),
    };
    let mut blend = D3D12_BLEND_DESC::default();
    blend.RenderTarget[0] = D3D12_RENDER_TARGET_BLEND_DESC {
        BlendEnable: blended.into(),
        LogicOpEnable: false.into(),
        SrcBlend: if straight_alpha() { D3D12_BLEND_SRC_ALPHA } else { D3D12_BLEND_ONE },
        DestBlend: D3D12_BLEND_INV_SRC_ALPHA,
        BlendOp: D3D12_BLEND_OP_ADD,
        SrcBlendAlpha: D3D12_BLEND_ONE,
        DestBlendAlpha: D3D12_BLEND_INV_SRC_ALPHA,
        BlendOpAlpha: D3D12_BLEND_OP_ADD,
        LogicOp: D3D12_LOGIC_OP_NOOP,
        RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
    };
    let mut formats = [DXGI_FORMAT_UNKNOWN; 8];
    formats[0] = format;
    // SAFETY: the root signature is inside the pixel shader; the
    // description points at live data and never releases its pointer.
    unsafe {
        let root: ID3D12RootSignature = device.CreateRootSignature(0, &ps[..])?;
        let desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::transmute_copy(&root),
            VS: code(&vs[..]),
            PS: code(&ps[..]),
            BlendState: blend,
            SampleMask: u32::MAX,
            RasterizerState: D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID,
                CullMode: D3D12_CULL_MODE_NONE,
                DepthClipEnable: true.into(),
                ..Default::default()
            },
            PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: 1,
            RTVFormats: formats,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            ..Default::default()
        };
        let pipeline: ID3D12PipelineState = device.CreateGraphicsPipelineState(&desc)?;
        Ok((root, pipeline))
    }
}

/// The format a presented texture is read as (UNORM rather than sRGB, so
/// the bytes reach the window as a copy would take them).
fn view_format(format: DXGI_FORMAT) -> Option<DXGI_FORMAT> {
    Some(DXGI_FORMAT(match format.0 {
        1..=4 => 2,
        9..=14 => 10,
        23..=25 => 24,
        26 => 26,
        27..=32 => 28,
        67 => 67,
        87 | 90 | 91 => 87,
        88 | 92 | 93 => 88,
        _ => return None,
    }))
}

/// Whether the layers can be drawn (the pass and its pipelines are there).
pub(crate) fn ready(format: DXGI_FORMAT) -> bool {
    pass().is_some_and(|p| p.pipeline(false, format).is_some() && p.pipeline(true, format).is_some())
}

/// Records into `list` the layers drawn into `back` (a `width` by `height`
/// back buffer of `format`, in the present state, left in it), each layer
/// left in its state.
pub(crate) fn record(
    host: &Host,
    list: &ID3D12GraphicsCommandList,
    back: &ID3D12Resource,
    layers: &[Layer],
    (width, height, format): (u32, u32, DXGI_FORMAT),
) {
    const READ: D3D12_RESOURCE_STATES = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
    let Some(pass) = pass() else { return };
    let mut moved: Vec<(&ID3D12Resource, D3D12_RESOURCE_STATES)> = Vec::new();
    for l in layers {
        let state = D3D12_RESOURCE_STATES(l.state as i32);
        if state.0 & READ.0 == 0 && !moved.iter().any(|(t, _)| t.as_raw() == l.texture.as_raw()) {
            moved.push((&l.texture, state));
        }
    }
    let mut to: Vec<_> = moved.iter().map(|(t, s)| transition(t, *s, READ)).collect();
    to.push(transition(back, D3D12_RESOURCE_STATE_PRESENT, D3D12_RESOURCE_STATE_RENDER_TARGET));
    let target = host.rtv(Some(back), None);
    let (fw, fh) = (width as f32, height as f32);
    // SAFETY: an open list; live resources; our views and pipelines live as
    // long as the process.
    unsafe {
        list.ResourceBarrier(&to);
        list.ClearRenderTargetView(target, &[0.0, 0.0, 0.0, 1.0], None);
        list.OMSetRenderTargets(1, Some(&target), false, None);
        list.SetDescriptorHeaps(&[Some(pass.views.clone())]);
        list.IASetPrimitiveTopology(windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY(4));
        for (i, l) in layers.iter().enumerate() {
            let desc = l.texture.GetDesc();
            let (Some(vf), 1) = (view_format(desc.Format), desc.SampleDesc.Count) else {
                continue;
            };
            let Some((root, pipeline)) = pass.pipeline(i > 0, format) else {
                continue;
            };
            let slot = pass.next.fetch_add(1, Ordering::Relaxed) % VIEWS;
            let view = D3D12_SHADER_RESOURCE_VIEW_DESC {
                Format: vf,
                ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2DARRAY,
                Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2DArray: D3D12_TEX2D_ARRAY_SRV {
                        MostDetailedMip: 0,
                        MipLevels: 1,
                        FirstArraySlice: 0,
                        ArraySize: 1,
                        ..Default::default()
                    },
                },
            };
            host.device.CreateShaderResourceView(
                &l.texture,
                Some(&view),
                D3D12_CPU_DESCRIPTOR_HANDLE { ptr: pass.cpu + (slot * pass.step) as usize },
            );
            let [sl, st, sr, sb] = l.texels(desc.Width as u32, desc.Height);
            let (w, h) = (desc.Width as f32, desc.Height as f32);
            let uv = [sl as f32 / w, st as f32 / h, sr as f32 / w, sb as f32 / h];
            let [dl, dt, dr, db] = l.fractions();
            list.SetGraphicsRootSignature(&root);
            list.SetPipelineState(&pipeline);
            list.SetGraphicsRoot32BitConstants(0, 4, uv.as_ptr().cast(), 0);
            list.SetGraphicsRootDescriptorTable(1, D3D12_GPU_DESCRIPTOR_HANDLE { ptr: pass.gpu + u64::from(slot * pass.step) });
            list.RSSetViewports(&[D3D12_VIEWPORT {
                TopLeftX: dl * fw,
                TopLeftY: dt * fh,
                Width: (dr - dl) * fw,
                Height: (db - dt) * fh,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]);
            list.RSSetScissorRects(&[RECT {
                left: (dl * fw).max(0.0) as i32,
                top: (dt * fh).max(0.0) as i32,
                right: (dr * fw).min(fw).ceil() as i32,
                bottom: (db * fh).min(fh).ceil() as i32,
            }]);
            list.DrawInstanced(3, 1, 0, 0);
        }
        let mut back_to: Vec<_> = moved.iter().map(|(t, s)| transition(t, READ, *s)).collect();
        back_to.push(transition(back, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_PRESENT));
        list.ResourceBarrier(&back_to);
    }
}

#[cfg(test)]
mod tests {
    use super::{fractions, texels};

    #[test]
    fn rectangles() {
        assert_eq!(texels(None, 1920, 1080), [0, 0, 1920, 1080]);
        assert_eq!(texels(Some([0, 0, 1280, 720]), 1920, 1080), [0, 0, 1280, 720]);
        assert_eq!(texels(Some([-5, 0, 4000, 720]), 1920, 1080), [0, 0, 1920, 720]);
        assert_eq!(fractions(None), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(fractions(Some([0.0, 0.0, 1920.0, 1080.0])), [0.0, 0.0, 1.0, 1.0]);
    }
}
