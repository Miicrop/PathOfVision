//! Description of the settings the generated settings window shows.
//! Labels and help texts match the board's former settings panel.
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SettingsSection {
    pub id: &'static str,
    pub name: &'static str,
    pub groups: Vec<SettingsGroup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsGroup {
    pub name: &'static str,
    /// Whether the group gets a "reset to defaults" button.
    pub reset: bool,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: FieldKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FieldKind {
    Toggle,
    /// `(value, label)` pairs.
    Choice { options: Vec<(&'static str, &'static str)> },
    /// `scale` converts the stored value to the displayed one (0.6 with scale 100 shows 60).
    Slider { min: f64, max: f64, step: f64, scale: f64, unit: &'static str },
    Number { min: i64, max: i64, step: i64, unit: &'static str },
    Path { placeholder: &'static str },
}

fn field(key: &'static str, label: &'static str, help: &'static str, kind: FieldKind) -> Field {
    Field { key, label, help, kind }
}

pub fn settings_schema() -> Vec<SettingsSection> {
    vec![
        SettingsSection {
            id: "general",
            name: "General",
            groups: vec![SettingsGroup {
                name: "App",
                reset: false,
                fields: vec![
                    field("autostart", "Start with Windows", "", FieldKind::Toggle),
                    field(
                        "client_log",
                        "Game log (Client.txt)",
                        "Only needed if Path of Exile is installed somewhere unusual.",
                        FieldKind::Path { placeholder: "found automatically" },
                    ),
                ],
            }],
        },
        SettingsSection {
            id: "altar",
            name: "Altars",
            groups: vec![
                SettingsGroup {
                    name: "Board cards",
                    reset: false,
                    fields: vec![
                        field(
                            "board_cards",
                            "Text",
                            "",
                            FieldKind::Choice {
                                options: vec![("short", "Short + text"), ("full", "Full text"), ("compact", "Compact")],
                            },
                        ),
                        field("board_target_tag", "Target tag", "", FieldKind::Toggle),
                    ],
                },
                SettingsGroup {
                    name: "Overlay",
                    reset: true,
                    fields: vec![
                        field(
                            "hud_font_size",
                            "Text size",
                            "icons scale with the text",
                            FieldKind::Slider { min: 10.0, max: 24.0, step: 1.0, scale: 1.0, unit: "px" },
                        ),
                        field(
                            "hud_tiers",
                            "Tiers",
                            "",
                            FieldKind::Choice { options: vec![("highlight", "Highlight"), ("off", "Off")] },
                        ),
                        field("hud_values", "Values", "", FieldKind::Toggle),
                        field(
                            "hud_other_opacity",
                            "Alpha",
                            "of the option that is not the pick (ADAPT - BRICK is never dimmed)",
                            FieldKind::Slider { min: 0.1, max: 1.0, step: 0.05, scale: 100.0, unit: "%" },
                        ),
                        field(
                            "hud_offset_x",
                            "Position X",
                            "",
                            FieldKind::Number { min: -3000, max: 3000, step: 10, unit: "px" },
                        ),
                        field(
                            "hud_bottom",
                            "Position Y",
                            "Anchor: bottom center of the game window. X moves right (negative: left), Y moves up.",
                            FieldKind::Number { min: 0, max: 2000, step: 5, unit: "px" },
                        ),
                    ],
                },
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{AltarSettings, General, clean_altar};

    fn section(id: &str) -> SettingsSection {
        settings_schema().into_iter().find(|s| s.id == id).unwrap()
    }

    fn keys(s: &SettingsSection) -> Vec<&'static str> {
        s.groups.iter().flat_map(|g| g.fields.iter().map(|f| f.key)).collect()
    }

    #[test]
    fn schema_keys_exist_in_settings() {
        let altar = serde_json::to_value(AltarSettings::default()).unwrap();
        for k in keys(&section("altar")) {
            assert!(altar.get(k).is_some(), "altar.{k}");
        }
        let general = serde_json::to_value(General::default()).unwrap();
        for k in keys(&section("general")) {
            assert!(general.get(k).is_some(), "general.{k}");
        }
        for f in section("altar").groups.iter().flat_map(|g| &g.fields) {
            let FieldKind::Choice { options } = &f.kind else { continue };
            for (value, _) in options {
                let mut a = AltarSettings::default();
                match f.key {
                    "board_cards" => a.board_cards = value.to_string(),
                    "hud_tiers" => a.hud_tiers = value.to_string(),
                    other => panic!("unexpected choice {other}"),
                }
                assert_eq!(clean_altar(a.clone()), a, "{}={value}", f.key);
            }
        }
    }

    #[test]
    fn schema_serializes_for_the_window() {
        let v = serde_json::to_value(settings_schema()).unwrap();
        assert_eq!(v[1]["groups"][1]["fields"][0]["kind"]["type"], "slider");
        assert_eq!(v[1]["groups"][0]["fields"][0]["kind"]["options"][0][0], "short");
        assert_eq!(v[0]["groups"][0]["fields"][1]["kind"]["type"], "path");
    }
}
