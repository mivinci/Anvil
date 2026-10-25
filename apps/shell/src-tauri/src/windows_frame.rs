//! Extend the Windows frame while leaving caption painting and input to DWM.
use windows_sys::Win32::{
  Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
  Graphics::{
    Dwm::{DwmDefWindowProc, DwmExtendFrameIntoClientArea},
    Gdi::{CombineRgn, CreateRectRgn, DeleteObject, RGN_DIFF, SetWindowRgn},
  },
  UI::{
    Controls::MARGINS,
    HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
    Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
    WindowsAndMessaging::{
      GW_CHILD, GW_HWNDNEXT, GetClientRect, GetWindow, GetWindowRect, IsZoomed, NCCALCSIZE_PARAMS,
      SM_CXPADDEDBORDER, SM_CYSIZEFRAME, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
      SWP_NOZORDER, SetWindowPos, WM_DPICHANGED, WM_NCCALCSIZE, WM_NCDESTROY, WM_SIZE,
    },
  },
};

const SUBCLASS: usize = 0x414e564c;

pub fn install(window: &tauri::WebviewWindow) -> tauri::Result<()> {
  let hwnd = window.hwnd()?.0 as HWND;
  // Setup runs on the window thread. The subclass stores no borrowed state.
  unsafe {
    if SetWindowSubclass(hwnd, Some(frame_proc), SUBCLASS, 0) == 0 {
      return Err(std::io::Error::last_os_error().into());
    }
    extend_frame(hwnd);
    SetWindowPos(
      hwnd,
      std::ptr::null_mut(),
      0,
      0,
      0,
      0,
      SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
    );
    clip_webviews(hwnd);
  }
  Ok(())
}

unsafe fn extend_frame(hwnd: HWND) {
  let height = (32 * unsafe { GetDpiForWindow(hwnd) } / 96) as i32;
  let margins = MARGINS {
    cxLeftWidth: 0,
    cxRightWidth: 0,
    cyTopHeight: height,
    cyBottomHeight: 0,
  };
  unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) };
}

// WebView2 is a child HWND. Excluding the native caption area from its region
// lets both DWM painting and non-client mouse input reach the parent frame.
unsafe fn clip_webviews(hwnd: HWND) {
  unsafe {
    let dpi = GetDpiForWindow(hwnd);
    let mut parent: RECT = std::mem::zeroed();
    if GetWindowRect(hwnd, &mut parent) == 0 {
      return;
    }
    let mut child = GetWindow(hwnd, GW_CHILD);
    while !child.is_null() {
      let mut bounds: RECT = std::mem::zeroed();
      let mut client: RECT = std::mem::zeroed();
      if GetWindowRect(child, &mut bounds) != 0 && GetClientRect(child, &mut client) != 0 {
        let region = CreateRectRgn(0, 0, client.right, client.bottom);
        let buttons = CreateRectRgn(
          parent.right - bounds.left - (138 * dpi / 96) as i32,
          parent.top - bounds.top,
          parent.right - bounds.left,
          parent.top - bounds.top + (32 * dpi / 96) as i32,
        );
        if !region.is_null() && !buttons.is_null() {
          if CombineRgn(region, region, buttons, RGN_DIFF) != 0 {
            // Windows owns the region only after a successful SetWindowRgn.
            if SetWindowRgn(child, region, 1) == 0 {
              DeleteObject(region);
            }
          } else {
            DeleteObject(region);
          }
        } else if !region.is_null() {
          DeleteObject(region);
        }
        if !buttons.is_null() {
          DeleteObject(buttons);
        }
      }
      child = GetWindow(child, GW_HWNDNEXT);
    }
  }
}

unsafe extern "system" fn frame_proc(
  hwnd: HWND,
  message: u32,
  wparam: WPARAM,
  lparam: LPARAM,
  _id: usize,
  _data: usize,
) -> LRESULT {
  unsafe {
    if message == WM_NCCALCSIZE && wparam != 0 {
      let params = &mut *(lparam as *mut NCCALCSIZE_PARAMS);
      let top = params.rgrc[0].top;
      // Preserve the standard side/bottom frame and its resize behavior.
      let result = DefSubclassProc(hwnd, message, wparam, lparam);
      params.rgrc[0].top = top;
      if IsZoomed(hwnd) != 0 {
        let dpi = GetDpiForWindow(hwnd);
        params.rgrc[0].top += GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi)
          + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
      }
      return result;
    }
    let mut result = 0;
    if DwmDefWindowProc(hwnd, message, wparam, lparam, &mut result) != 0 {
      return result;
    }
    let result = DefSubclassProc(hwnd, message, wparam, lparam);
    if message == WM_SIZE || message == WM_DPICHANGED {
      extend_frame(hwnd);
      clip_webviews(hwnd);
    } else if message == WM_NCDESTROY {
      RemoveWindowSubclass(hwnd, Some(frame_proc), SUBCLASS);
    }
    result
  }
}
