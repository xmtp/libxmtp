use std::path::Path;

use crate::AppWindow;
use slint::Weak;

pub fn open_file_dialog(handle: Weak<AppWindow>) {
    std::thread::spawn(move || {
        use native_dialog::FileDialog;
        if let Ok(Some(path)) = FileDialog::new()
            .set_title("Open Log File")
            .show_open_single_file()
        {
            let path_str = path.to_string_lossy().to_string();
            slint::invoke_from_event_loop(move || {
                if let Some(ui) = handle.upgrade() {
                    ui.invoke_file_selected(path_str.into());
                }
            })
            .ok();
        }
    });
}

pub fn file_selected(handle: Weak<AppWindow>, path: impl AsRef<Path>) {
    let path = path.as_ref();
    println!("Selected file: {path:?}");
}
