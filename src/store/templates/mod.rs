//! Model design templates: a model's design (fields, refs, rules living in it and,
//! optionally, sample records) as a versioned file that can be imported next to the
//! designs already here. See `docs/model-templates.md`.
//!
//! In a template, refs name their target model (`ref:trades`, not its id) and field ids
//! are dropped on import (every imported field gets a new one). Rules come in switched
//! off; sample records lose their ref values (the records they pointed at are elsewhere).
use super::models::{BARS, Design, Field, FieldType, Model, Plan, bar_builtins, slug};
use super::{ApplyOpts, NewRule, OwnerKind, RecordQuery, RuleSpec, Store};
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The `format` of a template file.
pub const FORMAT: &str = "muninn.model-template";
/// The newest template version this build reads and the one it writes.
pub const VERSION: u32 = 1;

/// A rule living in a template's model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateRule {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub spec: RuleSpec,
}

/// A model design template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub format: String,
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Refs name their model (`ref:NAME`).
    pub design: Design,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<TemplateRule>,
    /// Sample records: field name → value.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<Map<String, Value>>,
}

impl Template {
    pub fn new(name: &str, description: &str, design: Design) -> Self {
        Template {
            format: FORMAT.into(),
            version: VERSION,
            name: name.into(),
            description: description.into(),
            design,
            rules: vec![],
            records: vec![],
        }
    }

    /// Read a template file (JSON), checking its format and version.
    pub fn parse(bytes: &[u8]) -> Result<Template> {
        let v: Value = serde_json::from_slice(bytes).map_err(|e| Error::invalid("template", format!("not JSON ({e})")))?;
        if v.get("format").and_then(Value::as_str) != Some(FORMAT) {
            return Err(Error::invalid("template", format!("not a Muninn model template (no `\"format\": \"{FORMAT}\"`)")));
        }
        let t: Template = serde_json::from_value(v).map_err(|e| Error::invalid("template", format!("damaged ({e})")))?;
        if t.version == 0 || t.version > VERSION {
            return Err(Error::invalid("template", format!("version {} (this Muninn reads up to {VERSION}; upgrade to import it)", t.version)));
        }
        Ok(t)
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Names of the models its refs point at (besides itself).
    pub fn needs(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .design
            .fields
            .iter()
            .filter_map(|f| match &f.ty {
                FieldType::Ref(n) if n != &self.name => Some(n.clone()),
                _ => None,
            })
            .collect();
        v.dedup();
        v
    }
}

/// The templates shipped with Muninn.
pub fn builtin_templates() -> Vec<Template> {
    let mut bars = Template::new(
        BARS,
        "price bars: source, symbol, period, timestamp and OHLCV (the bars model's design)",
        Design { fields: bar_builtins(), builtins: false },
    );
    for f in bars.design.fields.iter_mut() {
        f.about = "price bar field".into();
    }
    vec![bars]
}

/// A built-in template by name (`bars`, or `builtin:bars`).
pub fn builtin_template(name: &str) -> Option<Template> {
    let n = name.strip_prefix("builtin:").unwrap_or(name);
    builtin_templates().into_iter().find(|t| t.name == n)
}

/// How to import a template.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportOpts {
    /// The new model's name (default: the template's).
    pub name: Option<String>,
    /// Add the template's missing fields to the model of that name instead of a new one.
    pub merge: bool,
    /// Leave the design as a draft to look at and apply later (`model apply` approves it).
    pub draft: bool,
    /// Skip the sample records.
    pub no_records: bool,
}

/// What importing a template would do.
#[derive(Debug, Clone, Serialize)]
pub struct ImportPlan {
    /// The model it lands in.
    pub model: String,
    pub merge: bool,
    pub plan: Plan,
    /// Merge: template fields not added because the model has a field of that name.
    pub kept: Vec<String>,
    pub records: usize,
    pub rules: usize,
}

/// What an import did.
#[derive(Debug, Clone, Serialize)]
pub struct Imported {
    pub model: Model,
    pub plan: Plan,
    pub records: usize,
    pub rules: Vec<String>,
    pub merged: bool,
}

impl Imported {
    pub fn summary(&self) -> String {
        let how = if self.merged { "merged into" } else { "imported as" };
        let state = if self.model.draft.is_some() {
            format!("a draft (`mnnn model apply {}` applies it)", self.model.name)
        } else {
            format!("v{}", self.model.version)
        };
        format!(
            "{how} model {} ({state}; {} step(s), {} sample record(s), {} rule(s) off)",
            self.model.name,
            self.plan.steps.len(),
            self.records,
            self.rules.len()
        )
    }
}

/// A free model name: `name`, else `name_2`, `name_3`, …
pub fn free_model_name(s: &Store, name: &str) -> Result<String> {
    let taken: Vec<String> = s.models()?.into_iter().map(|m| m.name).collect();
    let n = slug(name);
    if !taken.contains(&n) {
        return Ok(n);
    }
    Ok((2..).map(|i| format!("{n}_{i}")).find(|c| !taken.contains(c)).unwrap_or(n))
}

impl Store {
    /// A model's design as a template (`records`: with its records as samples).
    pub fn export_template(&self, key: &str, records: bool) -> Result<Template> {
        let m = self.find_model(key)?;
        let mut design = if m.is_bars() { m.design.clone().with_builtins() } else { m.design.clone() };
        design.builtins = false;
        for f in design.fields.iter_mut() {
            if let FieldType::Ref(id) = &f.ty {
                let n = if *id == m.id { m.name.clone() } else { self.model(id)?.map(|x| x.name).unwrap_or_else(|| id.clone()) };
                f.ty = FieldType::Ref(n);
            }
        }
        let mut t = Template::new(&m.name, &m.description, design);
        t.rules = self
            .owned_rules(OwnerKind::Model, &m.id)?
            .into_iter()
            .map(|r| TemplateRule { name: r.name, description: r.description, spec: r.spec })
            .collect();
        if records {
            if m.is_bars() {
                return Err(Error::invalid("template", "bars are not sample records; export bars with `mnnn export`"));
            }
            if m.version > 0 {
                let names: Vec<String> = m.design.fields.iter().map(|f| f.name.clone()).collect();
                t.records = self
                    .records(&m, &RecordQuery::default())?
                    .into_iter()
                    .map(|r| r.values.into_iter().filter(|(k, _)| names.contains(k)).collect())
                    .collect();
            }
        }
        Ok(t)
    }

    /// The template's design for model `id` (named `name`): new field ids, refs by id.
    fn resolve_design(&self, t: &Template, id: &str) -> Result<Design> {
        let mut fields = vec![];
        for f in &t.design.fields {
            let ty = match &f.ty {
                FieldType::Ref(n) if *n == t.name || n == id => FieldType::Ref(id.to_string()),
                FieldType::Ref(n) => FieldType::Ref(
                    self.find_model(n)
                        .map_err(|_| {
                            Error::invalid(
                                "template",
                                format!("field {} refers to model `{n}`, which is not here; import or create it first", f.name),
                            )
                        })?
                        .id,
                ),
                o => o.clone(),
            };
            fields.push(Field { ty, ..Field::new(&f.name, f.ty.clone()) }.with_constraints(f));
        }
        Ok(Design { fields, builtins: false })
    }

    /// The model a template would clash with (one of that name exists).
    pub fn template_clash(&self, t: &Template, opts: &ImportOpts) -> Result<Option<Model>> {
        let name = slug(opts.name.as_deref().unwrap_or(&t.name));
        Ok(self.models()?.into_iter().find(|m| m.name == name))
    }

    fn import_target(&self, t: &Template, opts: &ImportOpts) -> Result<(String, Option<Model>)> {
        let name = slug(opts.name.as_deref().unwrap_or(&t.name));
        match (self.template_clash(t, opts)?, opts.merge) {
            (Some(m), true) => Ok((name, Some(m))),
            (Some(m), false) => Err(Error::invalid(
                "template",
                format!(
                    "model `{}` already exists: import it under another name (--name {}) or merge into it (--merge)",
                    m.name,
                    free_model_name(self, &name)?
                ),
            )),
            (None, true) => Err(Error::invalid("template", format!("--merge: there is no model `{name}` to merge into"))),
            (None, false) => Ok((name, None)),
        }
    }

    /// A merged design: the model's (draft) design plus the template fields it lacks.
    fn merged(&self, t: &Template, m: &Model) -> Result<(Design, Vec<String>)> {
        let mut d = m.pending().clone();
        let mut kept = vec![];
        for f in self.resolve_design(t, &m.id)?.fields {
            match d.field(&f.name) {
                Some(o) if o.ty == f.ty => {}
                Some(_) => kept.push(f.name.clone()),
                None => d.fields.push(f),
            }
        }
        Ok((d, kept))
    }

    /// What importing a template would do (nothing is changed).
    pub fn plan_template(&self, t: &Template, opts: &ImportOpts) -> Result<ImportPlan> {
        let (name, target) = self.import_target(t, opts)?;
        let (m, design, kept) = match target {
            Some(m) => {
                let (d, kept) = self.merged(t, &m)?;
                (m, d, kept)
            }
            None => {
                let m = Model {
                    id: "new".into(),
                    name: name.clone(),
                    description: t.description.clone(),
                    builtin: false,
                    version: 0,
                    design: Design::default(),
                    draft: None,
                    author_id: None,
                    created_at: 0,
                    updated_at: 0,
                };
                let d = self.resolve_design(t, "new")?;
                self.check_design(&m, &d)?;
                (m, d, vec![])
            }
        };
        let plan = self.plan_model(&m, &design)?;
        let records = if opts.no_records || opts.draft || m.is_bars() { 0 } else { t.records.len() };
        Ok(ImportPlan { model: name, merge: opts.merge, plan, kept, records, rules: t.rules.len() })
    }

    /// Import a template: a new model (or a merge into one), applied unless `opts.draft`,
    /// with its sample records and its rules (switched off). `destructive` allows a merge
    /// plan that loses values.
    pub fn import_template(&self, t: &Template, opts: &ImportOpts, destructive: bool) -> Result<Imported> {
        let (name, target) = self.import_target(t, opts)?;
        let merged = target.is_some();
        let m = match target {
            Some(m) => {
                let (d, _) = self.merged(t, &m)?;
                self.set_model_draft(&m.id, Some(d))?
            }
            None => {
                let (m, _) = self.create_model(&name, &t.description, Design::default(), false)?;
                match self.resolve_design(t, &m.id).and_then(|d| self.set_model_draft(&m.id, Some(d))) {
                    Ok(m) => m,
                    Err(e) => {
                        let _ = self.remove_model(&m.id, true);
                        return Err(e);
                    }
                }
            }
        };
        let pending = m.draft.clone().unwrap_or_default();
        let plan = self.plan_model(&m, &pending)?;
        let m = if opts.draft {
            m
        } else {
            match self.apply_model(&m.id, &ApplyOpts { destructive, note: format!("template {}", t.name) }) {
                Ok((m, _)) => m,
                Err(e) => {
                    if !merged {
                        let _ = self.remove_model(&m.id, true);
                    }
                    return Err(e);
                }
            }
        };
        let mut records = 0;
        if !opts.draft && !opts.no_records && !m.is_bars() {
            for r in &t.records {
                let vals: Map<String, Value> = r
                    .iter()
                    .filter(|(k, v)| !v.is_null() && m.design.field(k).is_some_and(|f| !matches!(f.ty, FieldType::Ref(_))))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                self.create_record(&m, &vals)?;
                records += 1;
            }
        }
        let taken: Vec<String> = self.rules()?.into_iter().map(|r| r.name.to_lowercase()).collect();
        let mut rules = vec![];
        for r in &t.rules {
            let mut n = r.name.clone();
            if taken.contains(&n.to_lowercase()) {
                n = (1..)
                    .map(|i| if i == 1 { format!("{} ({})", r.name, m.name) } else { format!("{} ({}) {i}", r.name, m.name) })
                    .find(|c| !taken.contains(&c.to_lowercase()))
                    .unwrap_or(n);
            }
            let made = self.create_rule(&NewRule {
                name: n,
                description: r.description.clone(),
                owner_kind: OwnerKind::Model,
                owner_id: Some(m.id.clone()),
                spec: r.spec.clone(),
                enabled: false,
            })?;
            rules.push(made.name);
        }
        Ok(Imported { model: m, plan, records, rules, merged })
    }
}

impl Field {
    /// This field with `o`'s constraints and description.
    fn with_constraints(mut self, o: &Field) -> Field {
        self.required = o.required;
        self.unique = o.unique;
        self.index = o.index;
        self.default = o.default.clone();
        self.choices = o.choices.clone();
        self.about = o.about.clone();
        self
    }
}

/// A template from a model shared over the network (a model pack): refs to models other
/// than itself become text, and it is imported as a draft to approve.
pub fn from_pack(p: &super::archive::ModelPack) -> Result<Template> {
    let m = &p.model;
    if m.is_bars() {
        return Err(Error::invalid("template", "the bars model is built in; import the built-in bars template instead"));
    }
    let mut d = m.design.clone();
    for f in d.fields.iter_mut() {
        if let FieldType::Ref(id) = &f.ty {
            f.ty = if *id == m.id { FieldType::Ref(m.name.clone()) } else { FieldType::Text };
        }
    }
    let mut t = Template::new(&m.name, &m.description, d);
    t.records = p.records.iter().map(|r| r.values.clone()).collect();
    Ok(t)
}

#[cfg(test)]
mod tests;
