use cursive::theme::{BaseColor, Color, ColorStyle};
use std::sync::LazyLock;

pub static GREEN: LazyLock<ColorStyle> = LazyLock::new(|| {
    ColorStyle::new(Color::Dark(BaseColor::Green), Color::Dark(BaseColor::Black))
});

pub static LIGHT_GREEN: LazyLock<ColorStyle> = LazyLock::new(|| {
    ColorStyle::new(Color::Light(BaseColor::Green), Color::Dark(BaseColor::Black))
});

pub static BLUE: LazyLock<ColorStyle> = LazyLock::new(|| {
    ColorStyle::new(Color::Dark(BaseColor::Blue), Color::Dark(BaseColor::Black))
});

pub static LIGHT_BLUE: LazyLock<ColorStyle> = LazyLock::new(|| {
    ColorStyle::new(Color::Light(BaseColor::Blue), Color::Dark(BaseColor::Black))
});

pub static RED: LazyLock<ColorStyle> = LazyLock::new(|| {
    ColorStyle::new(Color::Dark(BaseColor::Red), Color::Dark(BaseColor::Black))
});

pub static WHITE: LazyLock<ColorStyle> = LazyLock::new(|| {
    ColorStyle::new(Color::Dark(BaseColor::White), Color::Dark(BaseColor::Black))
});

pub static YELLOW: LazyLock<ColorStyle> = LazyLock::new(|| {
    ColorStyle::new(Color::Dark(BaseColor::Yellow), Color::Dark(BaseColor::Black))
});

pub static MAGENTA: LazyLock<ColorStyle> = LazyLock::new(|| {
    ColorStyle::new(Color::Dark(BaseColor::Magenta), Color::Dark(BaseColor::Black))
});
