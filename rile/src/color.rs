/// A color.
///
/// Colors are stored with components normalised to the range `[0.0, 1.0]`,
/// matching the Lua implementation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Color {
    /// Red-green-blue, each channel in `[0, 1]`.
    Rgb { r: f64, g: f64, b: f64 },
    /// Cyan-magenta-yellow-key, each channel in `[0, 1]`.
    Cmyk { c: f64, m: f64, y: f64, k: f64 },
    /// Grayscale luminance in `[0, 1]`.
    Grayscale { l: f64 },
}

impl Color {
    /// Parse a color.
    ///
    /// Accepted formats:
    /// - Named CSS color (case-insensitive), e.g. `"rebeccapurple"`
    /// - `#RRGGBB` or `#RGB` hex
    /// - `"R G B"` — three numbers 0–255
    /// - `"R% G% B%"` — three percentages
    /// - `"C M Y K"` — four numbers 0–255
    /// - `"C% M% Y% K%"` — four percentages
    /// - `"L"` — single number 0–255 (grayscale)
    pub fn parse(input: &str) -> Result<Self, String> {
        if input.is_empty() {
            return Err("Not a color specification string (empty)".to_string());
        }

        // Named color lookup (case-insensitive)
        let lower = input.to_lowercase();
        if let Some(&[r, g, b]) = NAMED_COLORS.iter().find(|(n, _)| *n == lower).map(|(_, v)| v) {
            return Ok(Color::Rgb {
                r: r as f64 / 255.0,
                g: g as f64 / 255.0,
                b: b as f64 / 255.0,
            });
        }

        // #RRGGBB
        if let Some(hex) = input.strip_prefix('#') {
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16)
                    .map_err(|_| format!("Unparsable color {input}"))?;
                let g = u8::from_str_radix(&hex[2..4], 16)
                    .map_err(|_| format!("Unparsable color {input}"))?;
                let b = u8::from_str_radix(&hex[4..6], 16)
                    .map_err(|_| format!("Unparsable color {input}"))?;
                return Ok(Color::Rgb {
                    r: r as f64 / 255.0,
                    g: g as f64 / 255.0,
                    b: b as f64 / 255.0,
                });
            } else if hex.len() == 3 {
                let r = u8::from_str_radix(&hex[0..1], 16)
                    .map_err(|_| format!("Unparsable color {input}"))?;
                let g = u8::from_str_radix(&hex[1..2], 16)
                    .map_err(|_| format!("Unparsable color {input}"))?;
                let b = u8::from_str_radix(&hex[2..3], 16)
                    .map_err(|_| format!("Unparsable color {input}"))?;
                return Ok(Color::Rgb {
                    r: r as f64 / 15.0,
                    g: g as f64 / 15.0,
                    b: b as f64 / 15.0,
                });
            } else {
                return Err(format!("Unparsable color {input}"));
            }
        }

        // Percentage-separated tokens: strip trailing `%` from each
        let tokens: Vec<&str> = input.split_whitespace().collect();

        // 4 percentages → CMYK
        if tokens.len() == 4 && tokens.iter().all(|t| t.ends_with('%')) {
            let vals = parse_percent_tokens(&tokens)
                .map_err(|_| format!("Unparsable color {input}"))?;
            return Ok(Color::Cmyk {
                c: vals[0],
                m: vals[1],
                y: vals[2],
                k: vals[3],
            });
        }

        // 3 percentages → RGB
        if tokens.len() == 3 && tokens.iter().all(|t| t.ends_with('%')) {
            let vals = parse_percent_tokens(&tokens)
                .map_err(|_| format!("Unparsable color {input}"))?;
            return Ok(Color::Rgb {
                r: vals[0],
                g: vals[1],
                b: vals[2],
            });
        }

        // 4 plain numbers → CMYK (0–255)
        if tokens.len() == 4 {
            let vals = parse_plain_tokens(&tokens)
                .map_err(|_| format!("Unparsable color {input}"))?;
            return Ok(Color::Cmyk {
                c: vals[0] / 255.0,
                m: vals[1] / 255.0,
                y: vals[2] / 255.0,
                k: vals[3] / 255.0,
            });
        }

        // 3 plain numbers → RGB (0–255)
        if tokens.len() == 3 {
            let vals = parse_plain_tokens(&tokens)
                .map_err(|_| format!("Unparsable color {input}"))?;
            return Ok(Color::Rgb {
                r: vals[0] / 255.0,
                g: vals[1] / 255.0,
                b: vals[2] / 255.0,
            });
        }

        // Single number → grayscale (0–255)
        if tokens.len() == 1
            && let Ok(l) = tokens[0].parse::<f64>() {
                return Ok(Color::Grayscale { l: l / 255.0 });
            }

        Err(format!("Unparsable color {input}"))
    }
}

fn parse_percent_tokens(tokens: &[&str]) -> Result<Vec<f64>, ()> {
    tokens
        .iter()
        .map(|t| {
            t.trim_end_matches('%').parse::<f64>().map(|v| v / 100.0).map_err(|_| ())
        })
        .collect()
}

fn parse_plain_tokens(tokens: &[&str]) -> Result<Vec<f64>, ()> {
    tokens.iter().map(|t| t.parse::<f64>().map_err(|_| ())).collect()
}

impl std::str::FromStr for Color {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Color::parse(s)
    }
}

// ─── Named colors (CSS / SVG) ─────────────────────────────────────────────────
// Source: types/color.lua, values are (r, g, b) in 0–255.

static NAMED_COLORS: &[(&str, [u8; 3])] = &[
    ("aliceblue", [240, 248, 255]),
    ("antiquewhite", [250, 235, 215]),
    ("aqua", [0, 255, 255]),
    ("aquamarine", [127, 255, 212]),
    ("azure", [240, 255, 255]),
    ("beige", [245, 245, 220]),
    ("bisque", [255, 228, 196]),
    ("black", [0, 0, 0]),
    ("blanchedalmond", [255, 235, 205]),
    ("blue", [0, 0, 255]),
    ("blueviolet", [138, 43, 226]),
    ("brown", [165, 42, 42]),
    ("burlywood", [222, 184, 135]),
    ("cadetblue", [95, 158, 160]),
    ("chartreuse", [127, 255, 0]),
    ("chocolate", [210, 105, 30]),
    ("coral", [255, 127, 80]),
    ("cornflowerblue", [100, 149, 237]),
    ("cornsilk", [255, 248, 220]),
    ("crimson", [220, 20, 60]),
    ("cyan", [0, 255, 255]),
    ("darkblue", [0, 0, 139]),
    ("darkcyan", [0, 139, 139]),
    ("darkgoldenrod", [184, 134, 11]),
    ("darkgray", [169, 169, 169]),
    ("darkgreen", [0, 100, 0]),
    ("darkgrey", [169, 169, 169]),
    ("darkkhaki", [189, 183, 107]),
    ("darkmagenta", [139, 0, 139]),
    ("darkolivegreen", [85, 107, 47]),
    ("darkorange", [255, 140, 0]),
    ("darkorchid", [153, 50, 204]),
    ("darkred", [139, 0, 0]),
    ("darksalmon", [233, 150, 122]),
    ("darkseagreen", [143, 188, 143]),
    ("darkslateblue", [72, 61, 139]),
    ("darkslategray", [47, 79, 79]),
    ("darkslategrey", [47, 79, 79]),
    ("darkturquoise", [0, 206, 209]),
    ("darkviolet", [148, 0, 211]),
    ("deeppink", [255, 20, 147]),
    ("deepskyblue", [0, 191, 255]),
    ("dimgray", [105, 105, 105]),
    ("dimgrey", [105, 105, 105]),
    ("dodgerblue", [30, 144, 255]),
    ("firebrick", [178, 34, 34]),
    ("floralwhite", [255, 250, 240]),
    ("forestgreen", [34, 139, 34]),
    ("fuchsia", [255, 0, 255]),
    ("gainsboro", [220, 220, 220]),
    ("ghostwhite", [248, 248, 255]),
    ("gold", [255, 215, 0]),
    ("goldenrod", [218, 165, 32]),
    ("gray", [128, 128, 128]),
    ("grey", [128, 128, 128]),
    ("green", [0, 128, 0]),
    ("greenyellow", [173, 255, 47]),
    ("honeydew", [240, 255, 240]),
    ("hotpink", [255, 105, 180]),
    ("indianred", [205, 92, 92]),
    ("indigo", [75, 0, 130]),
    ("ivory", [255, 255, 240]),
    ("khaki", [240, 230, 140]),
    ("lavender", [230, 230, 250]),
    ("lavenderblush", [255, 240, 245]),
    ("lawngreen", [124, 252, 0]),
    ("lemonchiffon", [255, 250, 205]),
    ("lightblue", [173, 216, 230]),
    ("lightcoral", [240, 128, 128]),
    ("lightcyan", [224, 255, 255]),
    ("lightgoldenrodyellow", [250, 250, 210]),
    ("lightgray", [211, 211, 211]),
    ("lightgreen", [144, 238, 144]),
    ("lightgrey", [211, 211, 211]),
    ("lightpink", [255, 182, 193]),
    ("lightsalmon", [255, 160, 122]),
    ("lightseagreen", [32, 178, 170]),
    ("lightskyblue", [135, 206, 250]),
    ("lightslategray", [119, 136, 153]),
    ("lightslategrey", [119, 136, 153]),
    ("lightsteelblue", [176, 196, 222]),
    ("lightyellow", [255, 255, 224]),
    ("lime", [0, 255, 0]),
    ("limegreen", [50, 205, 50]),
    ("linen", [250, 240, 230]),
    ("magenta", [255, 0, 255]),
    ("maroon", [128, 0, 0]),
    ("mediumaquamarine", [102, 205, 170]),
    ("mediumblue", [0, 0, 205]),
    ("mediumorchid", [186, 85, 211]),
    ("mediumpurple", [147, 112, 219]),
    ("mediumseagreen", [60, 179, 113]),
    ("mediumslateblue", [123, 104, 238]),
    ("mediumspringgreen", [0, 250, 154]),
    ("mediumturquoise", [72, 209, 204]),
    ("mediumvioletred", [199, 21, 133]),
    ("midnightblue", [25, 25, 112]),
    ("mintcream", [245, 255, 250]),
    ("mistyrose", [255, 228, 225]),
    ("moccasin", [255, 228, 181]),
    ("navajowhite", [255, 222, 173]),
    ("navy", [0, 0, 128]),
    ("oldlace", [253, 245, 230]),
    ("olive", [128, 128, 0]),
    ("olivedrab", [107, 142, 35]),
    ("orange", [255, 165, 0]),
    ("orangered", [255, 69, 0]),
    ("orchid", [218, 112, 214]),
    ("palegoldenrod", [238, 232, 170]),
    ("palegreen", [152, 251, 152]),
    ("paleturquoise", [175, 238, 238]),
    ("palevioletred", [219, 112, 147]),
    ("papayawhip", [255, 239, 213]),
    ("peachpuff", [255, 218, 185]),
    ("peru", [205, 133, 63]),
    ("pink", [255, 192, 203]),
    ("plum", [221, 160, 221]),
    ("powderblue", [176, 224, 230]),
    ("purple", [128, 0, 128]),
    ("rebeccapurple", [102, 51, 153]),
    ("red", [255, 0, 0]),
    ("rosybrown", [188, 143, 143]),
    ("royalblue", [65, 105, 225]),
    ("saddlebrown", [139, 69, 19]),
    ("salmon", [250, 128, 114]),
    ("sandybrown", [244, 164, 96]),
    ("seagreen", [46, 139, 87]),
    ("seashell", [255, 245, 238]),
    ("sienna", [160, 82, 45]),
    ("silver", [192, 192, 192]),
    ("skyblue", [135, 206, 235]),
    ("slateblue", [106, 90, 205]),
    ("slategray", [112, 128, 144]),
    ("slategrey", [112, 128, 144]),
    ("snow", [255, 250, 250]),
    ("springgreen", [0, 255, 127]),
    ("steelblue", [70, 130, 180]),
    ("tan", [210, 180, 140]),
    ("teal", [0, 128, 128]),
    ("thistle", [216, 191, 216]),
    ("tomato", [255, 99, 71]),
    ("turquoise", [64, 224, 208]),
    ("violet", [238, 130, 238]),
    ("wheat", [245, 222, 179]),
    ("white", [255, 255, 255]),
    ("whitesmoke", [245, 245, 245]),
    ("yellow", [255, 255, 0]),
    ("yellowgreen", [154, 205, 50]),
];

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const REBECCA: Color = Color::Rgb { r: 0.4, g: 0.2, b: 0.6 };

    const REDDISH: Color = Color::Cmyk { c: 0.0, m: 0.81, y: 0.81, k: 0.3 };

    fn approx_eq_color(a: Color, b: Color) -> bool {
        match (a, b) {
            (Color::Rgb { r: r1, g: g1, b: b1 }, Color::Rgb { r: r2, g: g2, b: b2 }) => {
                (r1 - r2).abs() < 1e-9 && (g1 - g2).abs() < 1e-9 && (b1 - b2).abs() < 1e-9
            }
            (
                Color::Cmyk { c: c1, m: m1, y: y1, k: k1 },
                Color::Cmyk { c: c2, m: m2, y: y2, k: k2 },
            ) => {
                (c1 - c2).abs() < 1e-9
                    && (m1 - m2).abs() < 1e-9
                    && (y1 - y2).abs() < 1e-9
                    && (k1 - k2).abs() < 1e-9
            }
            (Color::Grayscale { l: l1 }, Color::Grayscale { l: l2 }) => (l1 - l2).abs() < 1e-9,
            _ => false,
        }
    }

    #[test]
    fn named_color_lowercase() {
        assert!(approx_eq_color(Color::parse("rebeccapurple").unwrap(), REBECCA));
    }

    #[test]
    fn named_color_mixed_case() {
        assert!(approx_eq_color(Color::parse("RebeccaPurple").unwrap(), REBECCA));
    }

    #[test]
    fn hex_six_digit() {
        assert!(approx_eq_color(Color::parse("#663399").unwrap(), REBECCA));
    }

    #[test]
    fn hex_three_digit() {
        assert!(approx_eq_color(Color::parse("#639").unwrap(), REBECCA));
    }

    #[test]
    fn rgb_plain_numbers() {
        assert!(approx_eq_color(Color::parse("102 51 153").unwrap(), REBECCA));
    }

    #[test]
    fn rgb_percentages() {
        assert!(approx_eq_color(Color::parse("40% 20% 60%").unwrap(), REBECCA));
    }

    #[test]
    fn cmyk_percentages() {
        assert!(approx_eq_color(Color::parse("0% 81% 81% 30%").unwrap(), REDDISH));
    }

    #[test]
    fn cmyk_plain_numbers() {
        assert!(approx_eq_color(
            Color::parse("0 206.55 206.55 76.5").unwrap(),
            REDDISH
        ));
    }

    #[test]
    fn grayscale_single_number() {
        let c = Color::parse("204").unwrap();
        assert!(approx_eq_color(c, Color::Grayscale { l: 0.8 }));
    }

    #[test]
    fn error_invalid_name() {
        assert!(Color::parse("not_a_color").is_err());
    }

    #[test]
    fn error_empty_string() {
        assert!(Color::parse("").is_err());
    }
}
