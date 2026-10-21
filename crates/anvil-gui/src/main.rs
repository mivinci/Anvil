slint::slint! {
    export component AnvilWindow inherits Window {
        title: "Anvil";
        width: 640px;
        height: 400px;
        Text {
            text: "Anvil\nJava management is available in the CLI";
            horizontal-alignment: center;
            vertical-alignment: center;
        }
    }
}

fn main() -> Result<(), slint::PlatformError> {
  AnvilWindow::new()?.run()
}
