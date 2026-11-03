use altar::Target;
use altar::verdict::*;
use std::path::Path;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn opt(t: Option<Target>, up: &[&str], down: &[&str]) -> OptionInput {
    OptionInput { target: t, upsides: s(up), downsides: s(down) }
}

fn profile() -> Profile {
    Profile::from_json(
        r#"{"name":"test","upsides":{"s":["u_s1","u_s2"],"a":["u_a"],"b":["u_b"]},
            "downsides":{"dangerous":["d_dan"],"brick":["d_brick"]}}"#,
    )
    .unwrap()
}

#[test]
fn verdict_table() {
    let p = profile();
    let rows = [
        ("u_s1", "d_brick", Verdict::Adapt),
        ("u_a", "d_brick", Verdict::Avoid),
        ("u_unrated", "d_brick", Verdict::Avoid),
        ("u_b", "d_dan", Verdict::Consider),
        ("u_s1", "d_dan", Verdict::Consider),
        ("u_unrated", "d_dan", Verdict::Avoid),
        ("u_b", "d_fine", Verdict::Take),
        ("u_unrated", "d_fine", Verdict::Neutral),
    ];
    for (up, down, want) in rows {
        let v = judge(&opt(Some(Target::Player), &[up], &[down]), &p);
        assert_eq!(v.verdict, want, "{up} {down}");
    }
}

#[test]
fn labels() {
    assert_eq!(Verdict::Adapt.label(), "ADAPT - BRICK");
    assert_eq!(Verdict::Take.label(), "TAKE");
    assert_eq!(Verdict::Consider.label(), "CONSIDER");
    assert_eq!(Verdict::Avoid.label(), "AVOID");
    assert_eq!(Verdict::Neutral.label(), "NEUTRAL");
    assert_eq!(Verdict::Unreadable.label(), "UNREADABLE");
}

#[test]
fn best_upside_and_worst_downside_count() {
    let v = judge(&opt(Some(Target::Boss), &["u_b", "u_s2"], &["d_fine", "d_dan", "d_brick"]), &profile());
    assert_eq!((v.up_tier, v.up_pos, v.down_tier, v.verdict), (Some(UpKind::Top), 1, Some(DownTier::Brick), Verdict::Adapt));
}

fn both(p: &Profile, top: OptionInput, bottom: OptionInput) -> Recommendation {
    let v = [judge(&top, p), judge(&bottom, p)];
    recommend(&[top, bottom], &v)
}

#[test]
fn take_prefers_higher_tier_then_column_position() {
    let p = profile();
    let r = both(&p, opt(Some(Target::Player), &["u_a"], &[]), opt(Some(Target::Boss), &["u_s2"], &[]));
    assert_eq!(r.choice, Some(Pos::Bottom));
    let r = both(&p, opt(Some(Target::Player), &["u_s2"], &[]), opt(Some(Target::Boss), &["u_s1"], &[]));
    assert_eq!(r.choice, Some(Pos::Bottom));
}

#[test]
fn risky_is_your_call_with_safe_other_option() {
    let r = both(&profile(), opt(Some(Target::Player), &["u_s1"], &["d_dan"]), opt(Some(Target::Boss), &["u_x"], &[]));
    assert_eq!((r.choice, r.about), (None, Some(Pos::Top)));
    assert_eq!(r.text, "Be careful. Bottom is safe.");
}

#[test]
fn adapt_text_without_safe_option() {
    let r = both(&profile(), opt(Some(Target::Player), &["u_x"], &["d_dan"]), opt(Some(Target::Boss), &["u_s1"], &["d_brick"]));
    assert_eq!((r.choice, r.about, r.text.as_str()), (None, Some(Pos::Bottom), "Brick downside, good upside."));
}

#[test]
fn both_avoid() {
    let r = both(&profile(), opt(Some(Target::Player), &["u_x"], &["d_dan"]), opt(Some(Target::Boss), &["u_a"], &["d_brick"]));
    assert_eq!((r.choice, r.text.as_str()), (None, "Leave this altar alone."));
}

#[test]
fn neutral_takes_minions() {
    let p = profile();
    let r = both(&p, opt(Some(Target::Player), &["u_x"], &[]), opt(Some(Target::Minions), &["u_y"], &[]));
    assert_eq!(r.choice, Some(Pos::Bottom));
    let r = both(&p, opt(Some(Target::Player), &["u_x"], &[]), opt(Some(Target::Boss), &["u_y"], &["d_dan"]));
    assert_eq!(r.choice, Some(Pos::Top));
}

#[test]
fn default_profile_loads() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/profiles/default.json");
    let p = Profile::load(&path).unwrap();
    assert!(p.read_only && !p.upsides.top.ids.is_empty());
}

#[test]
fn unreadable_option_blocks_the_recommendation() {
    let p = profile();
    let top = opt(None, &[], &[]);
    let bottom = opt(Some(Target::Player), &["u_s1"], &[]);
    let v = [judge(&top, &p), judge(&bottom, &p)];
    assert_eq!(v[0].verdict, Verdict::Unreadable);
    assert_eq!(recommend(&[top, bottom], &v).text, "Can't read both options yet.");
}

#[test]
fn old_profiles_are_upgraded_to_columns() {
    let p = Profile::from_json(r#"{"name":"o","upsides":{"s":["x"],"a":["y"],"b":["z"]}}"#).unwrap();
    assert_eq!(p.upsides.columns.len(), 2);
    assert_eq!((p.upsides.columns[0].name.as_str(), &p.upsides.columns[0].ids), ("B", &s(&["z"])));
    assert_eq!((p.upsides.columns[1].name.as_str(), &p.upsides.columns[1].ids), ("A", &s(&["y"])));
    assert_eq!((p.upsides.top.name.as_str(), &p.upsides.top.ids), ("S", &s(&["x"])));
}

#[test]
fn custom_columns_rank_by_position_and_only_top_adapts() {
    let p = Profile::from_json(
        r#"{"name":"cols","upsides":{"columns":[{"name":"Meh","ids":["u_meh"]},{"name":"Good","ids":["u_good"]},
            {"name":"Great","ids":["u_great"]}],"top":{"name":"Jackpot","ids":["u_top"]}},
            "downsides":{"dangerous":[],"brick":["d_brick"]}}"#,
    )
    .unwrap();
    let b = Some(Target::Boss);
    assert_eq!(judge(&opt(b, &["u_great"], &["d_brick"]), &p).verdict, Verdict::Avoid);
    assert_eq!(judge(&opt(b, &["u_top"], &["d_brick"]), &p).verdict, Verdict::Adapt);
    assert_eq!(judge(&opt(b, &["u_meh"], &[]), &p).verdict, Verdict::Take);
    let r = both(&p, opt(b, &["u_meh"], &[]), opt(Some(Target::Player), &["u_great"], &[]));
    assert_eq!(r.choice, Some(Pos::Bottom));
}

fn cols() -> Profile {
    Profile::from_json(
        r#"{"name":"cols","upsides":{"columns":[{"name":"Scarabs","ids":["u_scarab"]},{"name":"Strat","ids":["u_quant"]}],
            "top":{"name":"S","ids":["u_top"]}},"downsides":{"dangerous":["d_dan"],"brick":["d_brick"]}}"#,
    )
    .unwrap()
}

#[test]
fn consider_with_upside_further_right_beats_take() {
    let r = both(&cols(), opt(Some(Target::Player), &["u_quant"], &["d_dan"]), opt(Some(Target::Minions), &["u_scarab"], &[]));
    assert_eq!((r.choice, r.text.as_str()), (Some(Pos::Top), "Take the top option."));
}

#[test]
fn take_wins_over_consider_in_the_same_or_a_lower_column() {
    let p = cols();
    let r = both(&p, opt(Some(Target::Player), &["u_scarab"], &["d_dan"]), opt(Some(Target::Minions), &["u_scarab"], &[]));
    assert_eq!(r.choice, Some(Pos::Bottom));
    let r = both(&p, opt(Some(Target::Player), &["u_scarab"], &["d_dan"]), opt(Some(Target::Minions), &["u_quant"], &[]));
    assert_eq!(r.choice, Some(Pos::Bottom));
}

#[test]
fn key_column_is_treated_like_the_top_column() {
    let p = Profile::from_json(
        r#"{"name":"key","upsides":{"columns":[{"name":"Meh","ids":["u_meh"]},{"name":"Strat","ids":["u_quant"],"key":true}],
            "top":{"name":"S","ids":[]}},"downsides":{"dangerous":[],"brick":["d_brick"]}}"#,
    )
    .unwrap();
    assert!(p.upsides.columns[1].key && !p.upsides.columns[0].key);
    let v = judge(&opt(Some(Target::Player), &["u_quant"], &["d_brick"]), &p);
    assert_eq!((v.verdict, v.up_tier), (Verdict::Adapt, Some(UpKind::Top)));
    assert_eq!(judge(&opt(Some(Target::Player), &["u_meh"], &["d_brick"]), &p).verdict, Verdict::Avoid);
}

#[test]
fn profile_with_missing_parts_loads() {
    let p = Profile::from_json(r#"{"name": "x"}"#).unwrap();
    let v = judge(&OptionInput { target: Some(Target::Boss), upsides: vec!["NotAMod".into()], downsides: vec!["AlsoNot".into()] }, &p);
    assert_eq!(v.verdict, Verdict::Neutral);
}
