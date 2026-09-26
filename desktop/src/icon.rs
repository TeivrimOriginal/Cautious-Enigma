//! Иконка приложения, нарисованная кодом.
//!
//! Растровых файлов в репозитории нет, поэтому трей получает значок из
//! сгенерированного буфера: скруглённый квадрат фирменного цвета с белой
//! молнией — намёк на интервальное повторение.

/// Базовая сторона иконки, в этих координатах рисуется вся графика.
pub const BASE: u32 = 32;

/// Сторона иконки для трея.
pub const TRAY_SIZE: u32 = 32;

/// Цвет иконки в RGBA для трея.
pub fn tray_rgba() -> Vec<u8> {
    render(TRAY_SIZE)
}

/// Отрисовывает иконку заданного размера со сглаживанием по краям.
pub fn render(size: u32) -> Vec<u8> {
    let size = size.max(8);
    let scale = size as f32 / BASE as f32;
    let center = size as f32 / 2.0;
    let mut pixels = vec![0_u8; (size * size * 4) as usize];

    for row in 0..size {
        for column in 0..size {
            let x = column as f32 + 0.5;
            let y = row as f32 + 0.5;

            let distance = rounded_box(
                (x - center, y - center),
                (center - scale, center - scale),
                7.0 * scale,
            );
            let background = coverage(distance);
            if background <= 0.0 {
                continue;
            }

            // Вертикальный градиент фирменного синего.
            let ratio = (y / size as f32).clamp(0.0, 1.0);
            let mut color = [
                mix(0x5c, 0x3b, ratio),
                mix(0x7c, 0x5b, ratio),
                mix(0xfa, 0xdb, ratio),
            ];
            let bolt = bolt_coverage(x, y, scale);
            for channel in color.iter_mut() {
                *channel = mix(*channel, 255, bolt);
            }

            let offset = ((row * size + column) * 4) as usize;
            pixels[offset] = color[0];
            pixels[offset + 1] = color[1];
            pixels[offset + 2] = color[2];
            pixels[offset + 3] = (background * 255.0).round() as u8;
        }
    }

    pixels
}

/// Вершины молнии в координатах 32×32.
const BOLT: [(f32, f32); 6] = [
    (18.6, 3.6),
    (9.6, 18.0),
    (14.4, 18.0),
    (12.8, 28.4),
    (22.4, 13.6),
    (17.4, 13.6),
];

/// Покрытие пикселя молнией: четыре точки на пиксель против смаза.
fn bolt_coverage(x: f32, y: f32, scale: f32) -> f32 {
    let mut hits = 0;
    for offset_y in 0..2 {
        for offset_x in 0..2 {
            let px = (x - 0.25 + offset_x as f32 * 0.5) / scale;
            let py = (y - 0.25 + offset_y as f32 * 0.5) / scale;
            if inside_polygon((px, py)) {
                hits += 1;
            }
        }
    }
    hits as f32 / 4.0
}

/// Проверяет, что точка внутри многоугольника молнии.
fn inside_polygon(point: (f32, f32)) -> bool {
    let mut inside = false;
    let mut previous = BOLT.len() - 1;
    for (current, (xi, yi)) in BOLT.iter().copied().enumerate() {
        let (xj, yj) = BOLT[previous];
        let crosses = (yi > point.1) != (yj > point.1)
            && point.0 < (xj - xi) * (point.1 - yi) / (yj - yi) + xi;
        if crosses {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

/// Знаковое расстояние до скруглённого прямоугольника: меньше нуля — внутри.
fn rounded_box(point: (f32, f32), half: (f32, f32), radius: f32) -> f32 {
    let dx = point.0.abs() - half.0 + radius;
    let dy = point.1.abs() - half.1 + radius;
    let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
    outside + dx.max(dy).min(0.0) - radius
}

/// Покрытие пикселя фигурой: край шириной в один пиксель.
fn coverage(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Смешивание цветов: `from` в `to` по доле `ratio`.
fn mix(from: u8, to: u8, ratio: f32) -> u8 {
    (f32::from(from) + (f32::from(to) - f32::from(from)) * ratio)
        .round()
        .clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Пиксель иконки как `(r, g, b, a)`.
    fn pixel(pixels: &[u8], size: u32, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let offset = ((y * size + x) * 4) as usize;
        (
            pixels[offset],
            pixels[offset + 1],
            pixels[offset + 2],
            pixels[offset + 3],
        )
    }

    #[test]
    fn keeps_expected_size() {
        assert_eq!(tray_rgba().len(), (TRAY_SIZE * TRAY_SIZE * 4) as usize);
    }

    #[test]
    fn draws_transparent_corners_and_opaque_center() {
        let size = TRAY_SIZE;
        let pixels = render(size);
        assert_eq!(pixel(&pixels, size, 0, 0).3, 0, "угол вне иконки");
        assert_eq!(pixel(&pixels, size, size - 1, 0).3, 0, "угол вне иконки");
        assert_eq!(
            pixel(&pixels, size, size / 2, size / 2).3,
            255,
            "центр залит"
        );
    }

    #[test]
    fn draws_white_bolt_on_blue_background() {
        let size = TRAY_SIZE;
        let pixels = render(size);
        let white = (0..size)
            .flat_map(|y| (0..size).map(move |x| (x, y)))
            .filter(|(x, y)| pixel(&pixels, size, *x, *y) == (255, 255, 255, 255))
            .count();
        assert!(
            white > 20,
            "молния должна быть видна, светлых пикселей: {white}"
        );

        let (red, green, blue, alpha) = pixel(&pixels, size, 3, 3);
        assert!(alpha > 200, "фон в левом верхнем углу непрозрачный");
        assert!(blue > red && blue > green, "фон фирменного синего цвета");
    }

    #[test]
    fn scales_to_other_sizes() {
        assert_eq!(render(16).len(), (16 * 16 * 4) as usize);
        assert_eq!(render(64).len(), (64 * 64 * 4) as usize);
    }
}
