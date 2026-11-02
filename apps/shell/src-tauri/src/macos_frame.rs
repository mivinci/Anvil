//! Align native caption buttons in window coordinates, independent of linked SDK.
use objc2_app_kit::{NSWindow, NSWindowButton, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};

const TITLEBAR_HEIGHT: f64 = 32.0;
const LEFT_INSET: f64 = 12.0;

pub fn install(window: &tauri::WebviewWindow) -> tauri::Result<()> {
  align(window)?;
  let handle = window.clone();
  window.on_window_event(move |event| {
    if matches!(
      event,
      tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. }
    ) {
      // Tauri delivers native window events on the main event-loop thread.
      if let Err(error) = align(&handle) {
        tracing::warn!(%error, "Cannot align macOS caption buttons");
      }
    }
  });
  Ok(())
}

fn align(window: &tauri::WebviewWindow) -> tauri::Result<()> {
  let _main_thread = objc2::MainThreadMarker::new()
    .ok_or_else(|| std::io::Error::other("macOS caption layout requires the main thread"))?;
  let pointer = window.ns_window()?;
  // The Tauri window owns this NSWindow; no pointer escapes this synchronous call.
  let native = unsafe { &*pointer.cast::<NSWindow>() };
  if native.styleMask().contains(NSWindowStyleMask::FullScreen) {
    return Ok(());
  }
  let Some(content) = native.contentView() else {
    return Ok(());
  };
  content.layoutSubtreeIfNeeded();
  let top = content.convertRect_toView(content.bounds(), None);
  let target = NSPoint::new(0.0, top.origin.y + top.size.height - TITLEBAR_HEIGHT / 2.0);
  let Some(close) = native.standardWindowButton(NSWindowButton::CloseButton) else {
    return Ok(());
  };
  // Match the native titlebar hit region to our caption height as well. This is
  // the same AppKit container used by Wry's traffic-light inset implementation.
  if let Some(button_parent) = unsafe { close.superview() }
    && let Some(container) = unsafe { button_parent.superview() }
    && let Some(container_parent) = unsafe { container.superview() }
  {
    let desired = NSRect::new(
      NSPoint::new(
        top.origin.x,
        top.origin.y + top.size.height - TITLEBAR_HEIGHT,
      ),
      NSSize::new(top.size.width, TITLEBAR_HEIGHT),
    );
    let converted = container_parent.convertRect_fromView(desired, None);
    let mut frame = container.frame();
    if (frame.origin.y - converted.origin.y).abs() > 0.01
      || (frame.size.height - converted.size.height).abs() > 0.01
    {
      frame.origin.y = converted.origin.y;
      frame.size.height = converted.size.height;
      container.setFrame(frame);
    }
  }
  let close_x = close.frame().origin.x;
  for kind in [
    NSWindowButton::CloseButton,
    NSWindowButton::MiniaturizeButton,
    NSWindowButton::ZoomButton,
  ] {
    let Some(button) = native.standardWindowButton(kind) else {
      continue;
    };
    // AppKit caption buttons and their parent are only accessed on the main thread.
    let Some(parent) = (unsafe { button.superview() }) else {
      continue;
    };
    let frame = button.frame();
    let center = parent.convertPoint_fromView(target, None);
    let origin = NSPoint::new(
      LEFT_INSET + frame.origin.x - close_x,
      center.y - frame.size.height / 2.0,
    );
    if (frame.origin.x - origin.x).abs() > 0.01 || (frame.origin.y - origin.y).abs() > 0.01 {
      button.setFrameOrigin(origin);
    }
  }
  Ok(())
}
