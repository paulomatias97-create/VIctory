//! Spotify's own volume in the system mixer (what Windows' volume
//! mixer shows per app), on the default output device. Setting it changes
//! only how loud that app is; its audio is not touched.
//!
//! Windows remembers an app's level after it closes, so every level we
//! change is put back when the [`AppVolume`] goes (or with
//! [`AppVolume::restore`]).

use std::collections::HashMap;

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{
    IAudioSessionControl2, IAudioSessionManager2, IChannelAudioVolume, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator, eMultimedia, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::{Interface, PWSTR, Result};

use crate::apps::matches;

pub struct AppVolume {
    /// Each session's level before we changed it, by process id.
    saved: HashMap<u32, f32>,
    /// Each session's channel levels before we changed them (the balance).
    saved_channels: HashMap<u32, Vec<f32>>,
}

impl AppVolume {
    /// Prepares COM on this thread (multithreaded; fine if already so).
    pub fn new() -> Self {
        // SAFETY: plain COM initialisation; a second call on the thread
        // returns S_FALSE or RPC_E_CHANGED_MODE, both harmless here.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Self {
            saved: HashMap::new(),
            saved_channels: HashMap::new(),
        }
    }

    /// The audio sessions of the app `app` (a media app id), with their
    /// process ids.
    fn sessions(app: &str) -> Result<Vec<(u32, ISimpleAudioVolume)>> {
        // SAFETY: COM calls on interfaces we hold; the device enumerator is
        // created per call, nothing outlives it unsafely.
        unsafe {
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = devices.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
            let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
            let list = manager.GetSessionEnumerator()?;
            let mut out = Vec::new();
            for i in 0..list.GetCount()? {
                let control = list.GetSession(i)?;
                let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
                    continue;
                };
                let Ok(pid) = control2.GetProcessId() else {
                    continue;
                };
                if pid == 0 || !process_name(pid).is_some_and(|exe| matches(app, &exe)) {
                    continue;
                }
                if let Ok(volume) = control.cast::<ISimpleAudioVolume>() {
                    out.push((pid, volume));
                }
            }
            Ok(out)
        }
    }

    /// Sets the app's level (0 to 1), saving its own level the first time.
    /// Returns how many of its audio sessions took it (0: not playing
    /// through the default device, or not found).
    pub fn set(&mut self, app: &str, level: f32) -> usize {
        let level = level.clamp(0.0, 1.0);
        let Ok(sessions) = Self::sessions(app) else {
            return 0;
        };
        let mut n = 0;
        for (pid, volume) in sessions {
            // SAFETY: a live session's volume interface.
            unsafe {
                if !self.saved.contains_key(&pid)
                    && let Ok(own) = volume.GetMasterVolume()
                {
                    self.saved.insert(pid, own);
                }
                // The user's own level is the ceiling: ours scales it.
                let own = self.saved.get(&pid).copied().unwrap_or(1.0);
                if volume
                    .SetMasterVolume(own * level, std::ptr::null())
                    .is_ok()
                {
                    n += 1;
                }
            }
        }
        n
    }

    /// Sets the app's balance: `left` and `right` levels (0 to 1) of its
    /// channels (even ones left, odd ones right), on top of its own, saved
    /// the first time. Returns how many of its sessions took it.
    pub fn set_balance(&mut self, app: &str, left: f32, right: f32) -> usize {
        let Ok(sessions) = Self::sessions(app) else {
            return 0;
        };
        let mut n = 0;
        for (pid, volume) in sessions {
            let Ok(channels) = volume.cast::<IChannelAudioVolume>() else {
                continue;
            };
            // SAFETY: a live session's channel volume interface; indices
            // below its channel count.
            unsafe {
                let Ok(count) = channels.GetChannelCount() else {
                    continue;
                };
                let own = self.saved_channels.entry(pid).or_insert_with(|| {
                    (0..count)
                        .map(|i| channels.GetChannelVolume(i).unwrap_or(1.0))
                        .collect()
                });
                let mut ok = true;
                for i in 0..count {
                    let g = if i % 2 == 0 { left } else { right };
                    let base = own.get(i as usize).copied().unwrap_or(1.0);
                    ok &= channels
                        .SetChannelVolume(i, base * g.clamp(0.0, 1.0), std::ptr::null())
                        .is_ok();
                }
                n += usize::from(ok);
            }
        }
        n
    }

    /// Puts back a session's channel levels, if we changed them.
    fn restore_channels(&mut self, pid: u32, volume: &ISimpleAudioVolume) {
        let Some(own) = self.saved_channels.remove(&pid) else {
            return;
        };
        if let Ok(channels) = volume.cast::<IChannelAudioVolume>() {
            for (i, level) in own.into_iter().enumerate() {
                // SAFETY: a live session's channel volume interface.
                let _ = unsafe { channels.SetChannelVolume(i as u32, level, std::ptr::null()) };
            }
        }
    }

    /// Puts back every level changed for `app`.
    pub fn restore(&mut self, app: &str) {
        let Ok(sessions) = Self::sessions(app) else {
            return;
        };
        for (pid, volume) in sessions {
            self.restore_channels(pid, &volume);
            if let Some(own) = self.saved.remove(&pid) {
                // SAFETY: a live session's volume interface.
                let _ = unsafe { volume.SetMasterVolume(own, std::ptr::null()) };
            }
        }
    }

    /// Puts back every level we changed, whatever app it belonged to.
    fn restore_all(&mut self) {
        if self.saved.is_empty() && self.saved_channels.is_empty() {
            return;
        }
        // Any app: match every session by process id instead of name.
        // SAFETY: as in `sessions`.
        let all = unsafe {
            (|| -> Result<Vec<(u32, ISimpleAudioVolume)>> {
                let devices: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let device = devices.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
                let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
                let list = manager.GetSessionEnumerator()?;
                let mut out = Vec::new();
                for i in 0..list.GetCount()? {
                    let control = list.GetSession(i)?;
                    if let (Ok(c2), Ok(v)) = (
                        control.cast::<IAudioSessionControl2>(),
                        control.cast::<ISimpleAudioVolume>(),
                    ) && let Ok(pid) = c2.GetProcessId()
                    {
                        out.push((pid, v));
                    }
                }
                Ok(out)
            })()
        };
        for (pid, volume) in all.unwrap_or_default() {
            self.restore_channels(pid, &volume);
            if let Some(own) = self.saved.remove(&pid) {
                // SAFETY: a live session's volume interface.
                let _ = unsafe { volume.SetMasterVolume(own, std::ptr::null()) };
            }
        }
    }
}

impl Default for AppVolume {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AppVolume {
    fn drop(&mut self) {
        self.restore_all();
    }
}

/// The executable path of process `pid`.
fn process_name(pid: u32) -> Option<String> {
    // SAFETY: a query-only handle, closed before returning; the buffer and
    // its length go together.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}
