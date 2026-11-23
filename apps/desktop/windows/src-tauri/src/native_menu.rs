use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Emitter, Runtime,
};

const APP_NAME: &str = "SparrowShell";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppLocale {
    Zh,
    En,
}

impl AppLocale {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "zh" => Ok(Self::Zh),
            "en" => Ok(Self::En),
            _ => Err("Unsupported application locale".to_owned()),
        }
    }

    fn labels(self) -> MenuLabels {
        match self {
            Self::Zh => MenuLabels {
                about: "关于 SparrowShell",
                settings: "设置…",
                services: "服务",
                hide: "隐藏 SparrowShell",
                hide_others: "隐藏其他",
                show_all: "全部显示",
                quit: "退出 SparrowShell",
                file: "文件",
                close_window: "关闭窗口",
                edit: "编辑",
                undo: "撤销",
                redo: "重做",
                cut: "剪切",
                copy: "复制",
                paste: "粘贴",
                select_all: "全选",
            },
            Self::En => MenuLabels {
                about: "About SparrowShell",
                settings: "Settings…",
                services: "Services",
                hide: "Hide SparrowShell",
                hide_others: "Hide Others",
                show_all: "Show All",
                quit: "Quit SparrowShell",
                file: "File",
                close_window: "Close Window",
                edit: "Edit",
                undo: "Undo",
                redo: "Redo",
                cut: "Cut",
                copy: "Copy",
                paste: "Paste",
                select_all: "Select All",
            },
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct MenuLabels {
    about: &'static str,
    settings: &'static str,
    services: &'static str,
    hide: &'static str,
    hide_others: &'static str,
    show_all: &'static str,
    quit: &'static str,
    file: &'static str,
    close_window: &'static str,
    edit: &'static str,
    undo: &'static str,
    redo: &'static str,
    cut: &'static str,
    copy: &'static str,
    paste: &'static str,
    select_all: &'static str,
}

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    app.set_menu(build_menu(app, AppLocale::Zh)?).map(|_| ())
}

pub fn handle_event(app: &AppHandle, event: MenuEvent) {
    if let action @ ("about" | "settings") = event.id().as_ref() {
        let _ = app.emit("native-menu-action", action);
    }
}

#[tauri::command]
pub fn set_app_locale(app: AppHandle, locale: &str) -> Result<(), String> {
    let locale = AppLocale::parse(locale)?;
    app.set_menu(build_menu(&app, locale).map_err(|error| error.to_string())?)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, locale: AppLocale) -> tauri::Result<Menu<R>> {
    let text = locale.labels();
    let about = MenuItem::with_id(app, "about", text.about, true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", text.settings, true, Some("CmdOrCtrl+,"))?;

    #[cfg(target_os = "macos")]
    let app_menu = {
        let separator = PredefinedMenuItem::separator(app)?;
        let services = PredefinedMenuItem::services(app, Some(text.services))?;
        let hide = PredefinedMenuItem::hide(app, Some(text.hide))?;
        let hide_others = PredefinedMenuItem::hide_others(app, Some(text.hide_others))?;
        let show_all = PredefinedMenuItem::show_all(app, Some(text.show_all))?;
        let quit = PredefinedMenuItem::quit(app, Some(text.quit))?;
        Submenu::with_items(
            app,
            APP_NAME,
            true,
            &[
                &about,
                &PredefinedMenuItem::separator(app)?,
                &settings,
                &separator,
                &services,
                &PredefinedMenuItem::separator(app)?,
                &hide,
                &hide_others,
                &show_all,
                &PredefinedMenuItem::separator(app)?,
                &quit,
            ],
        )?
    };

    #[cfg(not(target_os = "macos"))]
    let app_menu = {
        let quit = PredefinedMenuItem::quit(app, Some(text.quit))?;
        Submenu::with_items(
            app,
            APP_NAME,
            true,
            &[
                &about,
                &PredefinedMenuItem::separator(app)?,
                &settings,
                &PredefinedMenuItem::separator(app)?,
                &quit,
            ],
        )?
    };

    let close_window = PredefinedMenuItem::close_window(app, Some(text.close_window))?;
    let file_menu = Submenu::with_items(app, text.file, true, &[&close_window])?;

    let undo = PredefinedMenuItem::undo(app, Some(text.undo))?;
    let redo = PredefinedMenuItem::redo(app, Some(text.redo))?;
    let cut = PredefinedMenuItem::cut(app, Some(text.cut))?;
    let copy = PredefinedMenuItem::copy(app, Some(text.copy))?;
    let paste = PredefinedMenuItem::paste(app, Some(text.paste))?;
    let select_all = PredefinedMenuItem::select_all(app, Some(text.select_all))?;
    let edit_menu = Submenu::with_items(
        app,
        text.edit,
        true,
        &[
            &undo,
            &redo,
            &PredefinedMenuItem::separator(app)?,
            &cut,
            &copy,
            &paste,
            &PredefinedMenuItem::separator(app)?,
            &select_all,
        ],
    )?;

    Menu::with_items(app, &[&app_menu, &file_menu, &edit_menu])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_copy_is_complete_and_uses_the_product_name() {
        let zh = AppLocale::Zh.labels();
        let en = AppLocale::En.labels();
        assert_eq!(zh.about, "关于 SparrowShell");
        assert_eq!(zh.file, "文件");
        assert_eq!(zh.paste, "粘贴");
        assert_eq!(en.about, "About SparrowShell");
        assert_eq!(en.file, "File");
        assert_eq!(en.paste, "Paste");
        assert!(!zh.quit.contains("sparrowshell-desktop"));
        assert!(!en.quit.contains("sparrowshell-desktop"));
    }

    #[test]
    fn locale_parser_rejects_unknown_values() {
        assert_eq!(AppLocale::parse("zh"), Ok(AppLocale::Zh));
        assert_eq!(AppLocale::parse("en"), Ok(AppLocale::En));
        assert!(AppLocale::parse("zh-CN").is_err());
        assert!(AppLocale::parse("").is_err());
    }
}
