//! Loading the DirectX 12 Agility SDK (`D3D12Core.dll`) at device creation
//! through `ID3D12SDKConfiguration1::CreateDeviceFactory`, without exporting
//! `D3D12SDKVersion` or `D3D12SDKPath` from the executable (#749).
//!
//! Looked for at `VICTORY_D3D12_AGILITY` if set, else `D3D12` beside the
//! executable. Each names a folder holding `D3D12Core.dll`, the DLL itself, or
//! an unpacked NuGet package. Without one, callers use the system's
//! `d3d12.dll`.

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use windows::Win32::Graphics::Direct3D::{D3D_FEATURE_LEVEL_12_0, ID3DBlob};
use windows::Win32::Graphics::Direct3D12::{
    CLSID_D3D12Debug, CLSID_D3D12DeviceRemovedExtendedData, CLSID_D3D12SDKConfiguration,
    D3D12_DEVICE_FACTORY_FLAG_ALLOW_RETURNING_EXISTING_DEVICE, D3D12_VERSIONED_ROOT_SIGNATURE_DESC,
    D3D12GetInterface, D3D12SerializeVersionedRootSignature, ID3D12Debug, ID3D12Device,
    ID3D12DeviceConfiguration, ID3D12DeviceFactory, ID3D12DeviceRemovedExtendedDataSettings,
    ID3D12SDKConfiguration1,
};
use windows::Win32::Graphics::Dxgi::IDXGIAdapter;
use windows::core::{Interface, PCSTR, Result as WinResult};

/// Serializes `desc` through `device`'s own `ID3D12DeviceConfiguration` when
/// available (so an Agility SDK device does not go through an older in-box
/// `d3d12.dll` that rejects 1.1 heap-indexing flags), falling back to
/// `D3D12SerializeVersionedRootSignature`.
///
/// # Safety
/// `desc` must point at live parameter and static-sampler arrays; `blob` and
/// `error` must be valid out pointers.
pub unsafe fn serialize_versioned_root_signature(
    device: &ID3D12Device,
    desc: *const D3D12_VERSIONED_ROOT_SIGNATURE_DESC,
    blob: *mut Option<ID3DBlob>,
    error: Option<*mut Option<ID3DBlob>>,
) -> WinResult<()> {
    if let Ok(cfg) = device.cast::<ID3D12DeviceConfiguration>() {
        // SAFETY: caller guarantees valid `desc` and out pointers.
        return unsafe { cfg.SerializeVersionedRootSignature(desc, blob, error) };
    }
    // SAFETY: as above.
    unsafe { D3D12SerializeVersionedRootSignature(desc, blob, error) }
}

/// A loaded Agility SDK core and its `ID3D12DeviceFactory`.
pub struct Factory {
    pub factory: ID3D12DeviceFactory,
    pub version: u32,
    pub dir: PathBuf,
}

impl Factory {
    /// Returns the process-wide Agility SDK factory, initialized once so
    /// concurrent `Host::new` callers share one loaded `D3D12Core.dll` and
    /// never race `CreateDeviceFactory` against `Release`. `Ok(None)` when
    /// `VICTORY_D3D12_AGILITY` is unset and no `D3D12\D3D12Core.dll` sits
    /// beside the executable.
    pub fn load() -> Result<Option<&'static Self>, String> {
        static FACTORY: OnceLock<Result<Option<Factory>, String>> = OnceLock::new();
        match FACTORY.get_or_init(|| match resolve_dir() {
            Some(dir) => Self::from_dir(&dir).map(Some),
            None => Ok(None),
        }) {
            Ok(opt) => Ok(opt.as_ref()),
            Err(e) => Err(e.clone()),
        }
    }

    /// Creates a device factory from the `D3D12Core.dll` in `input` (a
    /// directory, the DLL itself, or an unpacked NuGet package).
    pub fn from_dir(input: &Path) -> Result<Self, String> {
        let dll = locate_core(input).ok_or_else(|| {
            format!(
                "no D3D12Core.dll under {} (expected D3D12Core.dll or build\\native\\bin\\x64\\D3D12Core.dll)",
                input.display()
            )
        })?;
        let bytes = std::fs::read(&dll).map_err(|e| format!("reading {}: {e}", dll.display()))?;
        let version = sdk_version(&bytes)
            .ok_or_else(|| format!("{} does not export D3D12SDKVersion", dll.display()))?;
        let dir = dll
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", dll.display()))?
            .to_path_buf();

        // SAFETY: queries `d3d12.dll` for `ID3D12SDKConfiguration1` and asks it
        // for an independent device factory over `dir`.
        let factory = unsafe {
            let mut cfg: Option<ID3D12SDKConfiguration1> = None;
            D3D12GetInterface(&CLSID_D3D12SDKConfiguration, &mut cfg).map_err(|e| {
                format!("ID3D12SDKConfiguration1 unavailable ({e}); Windows 10 needs KB5029331+")
            })?;
            let cfg = cfg.ok_or("D3D12GetInterface returned no ID3D12SDKConfiguration1")?;
            let factory = create_factory(&cfg, version, &dir)?;
            let _ = factory.SetFlags(D3D12_DEVICE_FACTORY_FLAG_ALLOW_RETURNING_EXISTING_DEVICE);
            factory
        };
        Ok(Factory {
            factory,
            version,
            dir,
        })
    }

    /// Applies this factory to `d3d12.dll`'s global state when still unlocked,
    /// so title root-signature calls through `d3d12.dll` use the Agility core
    /// too. Ignored if global state was already locked.
    pub fn apply_global(&self) {
        // SAFETY: plain call on a live factory; failure only means global D3D12
        // state was already initialized elsewhere in the process.
        let _ = unsafe { self.factory.ApplyToGlobalState() };
    }

    /// The debug layer from this SDK (`d3d12SDKLayers.dll` beside
    /// `D3D12Core.dll`), configured on this factory before creating a device.
    pub fn debug(&self) -> WinResult<ID3D12Debug> {
        // SAFETY: plain configuration query on a live factory.
        unsafe { self.factory.GetConfigurationInterface(&CLSID_D3D12Debug) }
    }

    /// Device-removed extended data settings scoped to this factory.
    pub fn dred(&self) -> WinResult<ID3D12DeviceRemovedExtendedDataSettings> {
        // SAFETY: plain configuration query on a live factory.
        unsafe {
            self.factory
                .GetConfigurationInterface(&CLSID_D3D12DeviceRemovedExtendedData)
        }
    }

    /// Creates a `D3D_FEATURE_LEVEL_12_0` device on `adapter` (or the default
    /// hardware adapter when `None`) through this factory.
    pub fn create_device(&self, adapter: Option<&IDXGIAdapter>) -> WinResult<ID3D12Device> {
        let mut device: Option<ID3D12Device> = None;
        // SAFETY: valid factory and optional DXGI adapter.
        unsafe {
            match adapter {
                Some(a) => self
                    .factory
                    .CreateDevice(a, D3D_FEATURE_LEVEL_12_0, &mut device)?,
                None => self
                    .factory
                    .CreateDevice(None, D3D_FEATURE_LEVEL_12_0, &mut device)?,
            }
        }
        device.ok_or_else(windows::core::Error::empty)
    }
}

unsafe fn create_factory(
    cfg: &ID3D12SDKConfiguration1,
    version: u32,
    dir: &Path,
) -> Result<ID3D12DeviceFactory, String> {
    let mut candidates = Vec::new();
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        && let Some(rel) = relative_from(&exe_dir, dir)
    {
        candidates.push(with_trailing_sep(&rel));
    }
    let abs = with_trailing_sep(&dir.to_string_lossy());
    if !candidates.contains(&abs) {
        candidates.push(abs);
    }

    let mut last_err = String::new();
    for cand in &candidates {
        let Ok(cstr) = CString::new(cand.as_str()) else {
            continue;
        };
        // SAFETY: `cstr` is NUL-terminated and lives across the call.
        match unsafe {
            cfg.CreateDeviceFactory::<_, ID3D12DeviceFactory>(version, PCSTR(cstr.as_ptr().cast()))
        } {
            Ok(factory) => return Ok(factory),
            Err(e) => last_err = format!("CreateDeviceFactory(v{version}, {cand:?}) failed: {e}"),
        }
    }
    Err(last_err)
}

fn with_trailing_sep(s: &str) -> String {
    if s.is_empty() || s == "." {
        ".\\".to_owned()
    } else if s.ends_with('\\') || s.ends_with('/') {
        s.to_owned()
    } else {
        format!("{s}\\")
    }
}

/// Relative path from `base` to `target` when both share a prefix/drive.
fn relative_from(base: &Path, target: &Path) -> Option<String> {
    let base = std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    let target = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let b: Vec<_> = base.components().collect();
    let t: Vec<_> = target.components().collect();
    if b.first()? != t.first()? {
        return None;
    }
    let common = b.iter().zip(&t).take_while(|(x, y)| x == y).count();
    if common == 0 {
        return None;
    }
    let mut parts = Vec::new();
    for _ in common..b.len() {
        parts.push("..".to_owned());
    }
    for c in &t[common..] {
        parts.push(c.as_os_str().to_str()?.to_owned());
    }
    if parts.is_empty() {
        Some(".\\".to_owned())
    } else {
        Some(parts.join("\\"))
    }
}

fn resolve_dir() -> Option<PathBuf> {
    if let Some(env) = std::env::var_os("VICTORY_D3D12_AGILITY").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(env));
    }
    let beside = std::env::current_exe().ok()?.parent()?.join("D3D12");
    locate_core(&beside).map(|_| beside)
}

/// Finds `D3D12Core.dll` from a file path, folder, or unpacked NuGet root.
pub fn locate_core(input: &Path) -> Option<PathBuf> {
    if input.is_file() {
        return Some(input.to_path_buf());
    }
    for rel in ["D3D12Core.dll", r"build\native\bin\x64\D3D12Core.dll"] {
        let p = input.join(rel);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Reads the exported `u32` `D3D12SDKVersion` from a `D3D12Core.dll` PE image
/// without loading or executing the DLL.
pub fn sdk_version(pe: &[u8]) -> Option<u32> {
    let u16_at = |at: usize| {
        Some(u16::from_le_bytes(
            pe.get(at..at.checked_add(2)?)?.try_into().ok()?,
        ))
    };
    let u32_at = |at: usize| {
        Some(u32::from_le_bytes(
            pe.get(at..at.checked_add(4)?)?.try_into().ok()?,
        ))
    };

    if u16_at(0)? != 0x5A4D {
        return None;
    }
    let pe_off = u32_at(0x3C)? as usize;
    if pe.get(pe_off..pe_off.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let file_hdr = pe_off + 4;
    let num_sections = u16_at(file_hdr + 2)? as usize;
    let opt_size = u16_at(file_hdr + 16)? as usize;
    let opt = file_hdr + 20;
    let sections = opt.checked_add(opt_size)?;
    if sections.checked_add(num_sections.checked_mul(40)?)? > pe.len() {
        return None;
    }
    let dir_base = match u16_at(opt)? {
        0x20B if opt_size >= 120 => opt + 112,
        0x10B if opt_size >= 104 => opt + 96,
        _ => return None,
    };
    if u32_at(dir_base - 4)? == 0 {
        return None;
    }
    let export_rva = u32_at(dir_base)?;
    let export_size = u32_at(dir_base + 4)?;
    if export_rva == 0 || export_size < 40 {
        return None;
    }

    let rva_off = |rva: u32, len: usize| -> Option<usize> {
        for i in 0..num_sections {
            let s = sections + i * 40;
            let vsize = u32_at(s + 8)?;
            let va = u32_at(s + 12)?;
            let raw_size = u32_at(s + 16)?;
            let raw_ptr = u32_at(s + 20)?;
            let span = vsize.max(raw_size);
            if let Some(delta) = rva.checked_sub(va)
                && delta < span
                && (delta as usize).checked_add(len)? <= raw_size as usize
            {
                let off = (raw_ptr as usize).checked_add(delta as usize)?;
                if off.checked_add(len)? <= pe.len() {
                    return Some(off);
                }
            }
        }
        None
    };

    let exp = rva_off(export_rva, 40)?;
    let num_funcs = u32_at(exp + 20)? as usize;
    let num_names = u32_at(exp + 24)? as usize;
    let funcs_rva = u32_at(exp + 28)?;
    let names_rva = u32_at(exp + 32)?;
    let ords_rva = u32_at(exp + 36)?;
    let funcs = rva_off(funcs_rva, num_funcs.checked_mul(4)?)?;
    let names = rva_off(names_rva, num_names.checked_mul(4)?)?;
    let ords = rva_off(ords_rva, num_names.checked_mul(2)?)?;

    const TARGET: &[u8] = b"D3D12SDKVersion\0";
    for i in 0..num_names {
        let name_rva = u32_at(names + i * 4)?;
        let Some(name_off) = rva_off(name_rva, TARGET.len()) else {
            continue;
        };
        if &pe[name_off..name_off + TARGET.len()] != TARGET {
            continue;
        }
        let ord = u16_at(ords + i * 2)? as usize;
        if ord >= num_funcs {
            return None;
        }
        let target_rva = u32_at(funcs + ord * 4)?;
        if (export_rva..export_rva.saturating_add(export_size)).contains(&target_rva) {
            return None;
        }
        return u32_at(rva_off(target_rva, 4)?);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal synthetic PE32+ image whose export table exports
    /// `name` pointing at a 4-byte constant `value` in `.rdata`.
    fn synthetic_pe(name: &[u8], value: u32, forwarded: bool) -> Vec<u8> {
        let mut pe = vec![0u8; 0x600];
        pe[0..2].copy_from_slice(&0x5A4Du16.to_le_bytes());
        pe[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        pe[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
        let opt = 0x98;
        pe[opt..opt + 2].copy_from_slice(&0x20Bu16.to_le_bytes());
        pe[opt + 108..opt + 112].copy_from_slice(&16u32.to_le_bytes());
        pe[opt + 112..opt + 116].copy_from_slice(&0x1000u32.to_le_bytes());
        pe[opt + 116..opt + 120].copy_from_slice(&0x80u32.to_le_bytes());

        let sec = opt + 240;
        pe[sec..sec + 6].copy_from_slice(b".rdata");
        pe[sec + 8..sec + 12].copy_from_slice(&0x200u32.to_le_bytes());
        pe[sec + 12..sec + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        pe[sec + 16..sec + 20].copy_from_slice(&0x200u32.to_le_bytes());
        pe[sec + 20..sec + 24].copy_from_slice(&0x200u32.to_le_bytes());

        let exp = 0x200;
        pe[exp + 20..exp + 24].copy_from_slice(&1u32.to_le_bytes());
        pe[exp + 24..exp + 28].copy_from_slice(&1u32.to_le_bytes());
        pe[exp + 28..exp + 32].copy_from_slice(&0x1080u32.to_le_bytes());
        pe[exp + 32..exp + 36].copy_from_slice(&0x1084u32.to_le_bytes());
        pe[exp + 36..exp + 40].copy_from_slice(&0x1088u32.to_le_bytes());

        let target_rva: u32 = if forwarded { 0x1040 } else { 0x1100 };
        pe[0x280..0x284].copy_from_slice(&target_rva.to_le_bytes());
        pe[0x284..0x288].copy_from_slice(&0x1090u32.to_le_bytes());
        pe[0x288..0x28A].copy_from_slice(&0u16.to_le_bytes());
        pe[0x290..0x290 + name.len()].copy_from_slice(name);
        pe[0x300..0x304].copy_from_slice(&value.to_le_bytes());
        pe
    }

    #[test]
    fn reads_exported_sdk_version_and_rejects_bad_images() {
        let pe = synthetic_pe(b"D3D12SDKVersion\0", 620, false);
        assert_eq!(sdk_version(&pe), Some(620));

        let other = synthetic_pe(b"D3D12GetInterface\0", 620, false);
        assert_eq!(sdk_version(&other), None);

        let forwarded = synthetic_pe(b"D3D12SDKVersion\0", 620, true);
        assert_eq!(sdk_version(&forwarded), None);

        assert_eq!(sdk_version(&[]), None);
        assert_eq!(sdk_version(&pe[..0x100]), None);
    }

    #[test]
    fn locates_core_and_formats_relative_paths() {
        let tmp = std::env::temp_dir().join(format!("victory-agility-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let bin = tmp.join(r"build\native\bin\x64");
        let sibling = tmp.join(r"app\bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        let dll = bin.join("D3D12Core.dll");
        std::fs::write(&dll, synthetic_pe(b"D3D12SDKVersion\0", 620, false)).unwrap();

        assert_eq!(locate_core(&dll).as_deref(), Some(dll.as_path()));
        assert_eq!(locate_core(&bin).as_deref(), Some(dll.as_path()));
        assert_eq!(locate_core(&tmp).as_deref(), Some(dll.as_path()));
        assert_eq!(
            with_trailing_sep(&relative_from(&sibling, &bin).unwrap()),
            r"..\..\build\native\bin\x64\"
        );
        assert_eq!(relative_from(&bin, &bin).as_deref(), Some(".\\"));
        assert_eq!(
            relative_from(Path::new(r"C:\app"), Path::new(r"D:\agility")),
            None
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Live host test when `VICTORY_D3D12_AGILITY` is set: loads the Agility
    /// core through `ID3D12SDKConfiguration1::CreateDeviceFactory`, checks the
    /// active device SDK version, and serializes + creates a `0xc01` 1.1 root
    /// signature through `ID3D12DeviceConfiguration` (#749).
    #[test]
    fn creates_device_and_serializes_heap_indexed_root_signature_when_configured() {
        use windows::Win32::Graphics::Direct3D12::*;
        if std::env::var_os("VICTORY_D3D12_AGILITY")
            .filter(|v| !v.is_empty())
            .is_none()
        {
            return;
        }
        let _one = crate::emit::valu_tests::ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let f = Factory::load()
            .expect("Agility factory")
            .expect("VICTORY_D3D12_AGILITY set");
        let device = f.create_device(None).expect("Agility device");
        let cfg: ID3D12DeviceConfiguration = device.cast().expect("ID3D12DeviceConfiguration");
        // SAFETY: plain query on a live device configuration.
        assert_eq!(unsafe { cfg.GetDesc() }.SDKVersion, f.version);

        let desc = D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
            Version: D3D_ROOT_SIGNATURE_VERSION_1_1,
            Anonymous: D3D12_VERSIONED_ROOT_SIGNATURE_DESC_0 {
                Desc_1_1: D3D12_ROOT_SIGNATURE_DESC1 {
                    Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT
                        | D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED
                        | D3D12_ROOT_SIGNATURE_FLAG_SAMPLER_HEAP_DIRECTLY_INDEXED,
                    ..Default::default()
                },
            },
        };
        // SAFETY: serializes an empty 1.1 root signature with 0xc01 flags via
        // the device's own configuration and creates it on the Agility device.
        unsafe {
            let mut blob = None;
            serialize_versioned_root_signature(&device, &desc, &mut blob, None)
                .expect("serialize 0xc01");
            let blob = blob.unwrap();
            let bytes = std::slice::from_raw_parts(
                blob.GetBufferPointer().cast::<u8>(),
                blob.GetBufferSize(),
            );
            let _: ID3D12RootSignature = device
                .CreateRootSignature(0, bytes)
                .expect("0xc01 root signature on Agility device");
        }
    }
}
