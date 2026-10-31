use super::*;
use crate::designer::parse_fields;
use crate::store::Then;
use serde_json::json;

fn store_with_trades() -> Store {
    let s = Store::open_in_memory().unwrap();
    let (m, _) = s.create_model("venues", "", Design::default(), false).unwrap();
    let d = Design { fields: parse_fields(&s, &m, "name:text required").unwrap(), ..Default::default() };
    s.set_model_draft(&m.id, Some(d)).unwrap();
    s.apply_model(&m.id, &ApplyOpts::default()).unwrap();
    let (m, _) = s.create_model("trades", "my trades", Design::default(), false).unwrap();
    let d = Design {
        fields: parse_fields(&s, &m, "price:real required default=0; side:text choices=buy,sell; venue:ref:venues; parent:ref:trades").unwrap(),
        ..Default::default()
    };
    s.set_model_draft(&m.id, Some(d)).unwrap();
    let (m, _) = s.apply_model(&m.id, &ApplyOpts::default()).unwrap();
    s.create_record(&m, &json!({"price": 3.5, "side": "buy"}).as_object().unwrap().clone()).unwrap();
    let spec = RuleSpec { on: vec!["*".into()], then: vec![Then::Status { text: "x".into() }], ..Default::default() };
    s.create_rule(&NewRule {
        name: "watch".into(),
        owner_kind: OwnerKind::Model,
        owner_id: Some(m.id.clone()),
        spec,
        enabled: true,
        ..Default::default()
    })
    .unwrap();
    s
}

#[test]
fn export_names_refs_and_carries_rules_and_samples() {
    let s = store_with_trades();
    let t = s.export_template("trades", true).unwrap();
    assert_eq!((t.format.as_str(), t.version), (FORMAT, VERSION));
    let ty = |n: &str| t.design.field(n).unwrap().ty.clone();
    assert_eq!(ty("venue"), FieldType::Ref("venues".into()));
    assert_eq!(ty("parent"), FieldType::Ref("trades".into()));
    assert_eq!(t.rules.len(), 1);
    assert_eq!(t.records.len(), 1);
    assert_eq!(t.records[0]["price"], json!(3.5));
    assert_eq!(t.needs(), vec!["venues".to_string()]);
    let back = Template::parse(t.to_json().unwrap().as_bytes()).unwrap();
    assert_eq!(back, t);
    assert!(s.export_template("trades", false).unwrap().records.is_empty());
}

#[test]
fn parse_checks_format_and_version() {
    assert!(Template::parse(b"{}").is_err());
    assert!(Template::parse(b"nope").is_err());
    let mut t = Template::new("x", "", Design::default());
    t.version = VERSION + 1;
    let e = Template::parse(t.to_json().unwrap().as_bytes()).unwrap_err().to_string();
    assert!(e.contains("upgrade"), "{e}");
}

#[test]
fn import_plans_first_then_makes_the_model() {
    let s = store_with_trades();
    let t = s.export_template("trades", true).unwrap();
    let opts = ImportOpts { name: Some("trades2".into()), ..Default::default() };
    let p = s.plan_template(&t, &opts).unwrap();
    assert_eq!(p.model, "trades2");
    assert!(!p.plan.steps.is_empty());
    assert_eq!((p.records, p.rules), (1, 1));
    assert!(s.find_model("trades2").is_err(), "planning changes nothing");
    let done = s.import_template(&t, &opts, false).unwrap();
    assert_eq!(done.model.version, 1);
    assert_eq!(done.records, 1);
    assert_eq!(done.rules, vec!["watch (trades2)".to_string()]);
    let m = s.find_model("trades2").unwrap();
    assert_eq!(m.design.field("parent").unwrap().ty, FieldType::Ref(m.id.clone()));
    assert_eq!(m.design.field("venue").unwrap().ty, FieldType::Ref(s.find_model("venues").unwrap().id));
    assert_eq!(s.count_records(&m).unwrap(), 1);
    let rules = s.owned_rules(OwnerKind::Model, &m.id).unwrap();
    assert!(!rules[0].enabled, "imported rules start switched off");
}

#[test]
fn clashes_ask_for_a_rename_or_merge() {
    let s = store_with_trades();
    let mut t = s.export_template("trades", false).unwrap();
    let e = s.plan_template(&t, &ImportOpts::default()).unwrap_err().to_string();
    assert!(e.contains("--name trades_2") && e.contains("--merge"), "{e}");
    t.design.fields.push(Field::new("fee", FieldType::Real));
    t.design.fields.push(Field::new("side", FieldType::Integer));
    let opts = ImportOpts { merge: true, ..Default::default() };
    let p = s.plan_template(&t, &opts).unwrap();
    assert_eq!(p.kept, vec!["side".to_string()]);
    let done = s.import_template(&t, &opts, false).unwrap();
    assert!(done.merged);
    let m = s.find_model("trades").unwrap();
    assert_eq!(m.version, 2);
    assert!(m.design.field("fee").is_some());
    assert_eq!(m.design.field("side").unwrap().ty, FieldType::Text);
    assert_eq!(s.count_records(&m).unwrap(), 1, "merge keeps the records");
    assert!(s.plan_template(&t, &ImportOpts { name: Some("nothing".into()), merge: true, ..Default::default() }).is_err());
}

#[test]
fn missing_ref_targets_are_named() {
    let s = store_with_trades();
    let t = s.export_template("trades", false).unwrap();
    let fresh = Store::open_in_memory().unwrap();
    let e = fresh.plan_template(&t, &ImportOpts::default()).unwrap_err().to_string();
    assert!(e.contains("venues"), "{e}");
}

#[test]
fn drafts_wait_for_apply() {
    let s = store_with_trades();
    let t = s.export_template("trades", true).unwrap();
    let done = s.import_template(&t, &ImportOpts { name: Some("t3".into()), draft: true, ..Default::default() }, false).unwrap();
    assert_eq!(done.model.version, 0);
    assert!(done.model.draft.is_some());
    assert_eq!(done.records, 0);
    assert!(done.summary().contains("mnnn model apply t3"));
}

#[test]
fn builtin_bars_template_matches_the_bars_model() {
    let s = Store::open_in_memory().unwrap();
    let t = builtin_template("builtin:bars").unwrap();
    let names: Vec<&str> = t.design.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, crate::store::models::BAR_FIELDS.iter().map(|(n, _)| *n).collect::<Vec<_>>());
    let exported = s.export_template("bars", false).unwrap();
    let en: Vec<&str> = exported.design.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(en, names);
    let done = s.import_template(&t, &ImportOpts { name: Some("my_bars".into()), ..Default::default() }, false).unwrap();
    assert_eq!(done.model.design.fields.len(), names.len());
    assert!(done.model.design.fields.iter().all(|f| !f.id.starts_with("bar.")));
    assert!(s.export_template("bars", true).is_err());
}

#[test]
fn packs_from_peers_become_templates() {
    let s = store_with_trades();
    let m = s.find_model("trades").unwrap();
    let records = s.records(&m, &RecordQuery::default()).unwrap();
    let p = crate::store::archive::ModelPack { model: m, versions: vec![], records };
    let t = from_pack(&p).unwrap();
    assert_eq!(t.design.field("venue").unwrap().ty, FieldType::Text);
    assert_eq!(t.design.field("parent").unwrap().ty, FieldType::Ref("trades".into()));
    assert_eq!(t.records.len(), 1);
}
