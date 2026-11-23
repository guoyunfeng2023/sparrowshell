use tauri::{utils::config::Color, WebviewWindow};

fn theme_background(theme: &str) -> Result<Color, String> {
    match theme {
        "light" => Ok(Color(255, 255, 255, 255)),
        "dark" => Ok(Color(13, 16, 18, 255)),
        _ => Err("不支持的主题".to_owned()),
    }
}

#[tauri::command]
pub fn set_app_theme(window: WebviewWindow, theme: String) -> Result<(), String> {
    let background = theme_background(&theme)?;
    window
        .set_background_color(Some(background))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::theme_background;

    #[test]
    fn native_background_matches_the_web_theme() {
        let light = theme_background("light").unwrap();
        assert_eq!((light.0, light.1, light.2, light.3), (255, 255, 255, 255));
        let dark = theme_background("dark").unwrap();
        assert_eq!((dark.0, dark.1, dark.2, dark.3), (13, 16, 18, 255));
    }

    #[test]
    fn unknown_themes_do_not_change_the_window() {
        assert!(theme_background("blue").is_err());
    }
}
