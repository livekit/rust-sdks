use std::{collections::BTreeSet, path::Path};

use anyhow::Context;

use crate::record::Settings;

const JUDGING: [&str; 3] = ["slo", "limits", "scoring"];

pub fn parse(text: &str) -> anyhow::Result<toml::Table> {
    let table: toml::Table = toml::from_str(text)?;
    check_keys(&table, &known_keys()?, "")?;
    toml::from_str::<Settings>(text)?;
    Ok(table)
}

fn known_keys() -> anyhow::Result<BTreeSet<String>> {
    let mut every = Settings { tile_height: Some(0), ..Settings::default() };
    every.slo.video_score_floor = Some(0.0);
    every.slo.delay_ceiling_ms = Some(0.0);
    Ok(dotted_keys(&toml::Table::try_from(every)?))
}

fn check_keys(table: &toml::Table, known: &BTreeSet<String>, prefix: &str) -> anyhow::Result<()> {
    for (key, value) in table {
        let path = format!("{prefix}{key}");
        let section = format!("{path}.");
        let is_section = known.iter().any(|k| k.starts_with(&section));
        match value {
            toml::Value::Table(inner) if is_section => check_keys(inner, known, &section)?,
            _ if is_section || known.contains(&path) => {}
            _ => {
                let siblings: BTreeSet<&str> = known
                    .iter()
                    .filter_map(|k| k.strip_prefix(prefix))
                    .map(|rest| rest.split_once('.').map_or(rest, |(head, _)| head))
                    .collect();
                let siblings: Vec<&str> = siblings.into_iter().collect();
                anyhow::bail!("unknown key {path} (expected one of {})", siblings.join(", "));
            }
        }
    }
    Ok(())
}

fn dotted_keys(table: &toml::Table) -> BTreeSet<String> {
    table
        .iter()
        .flat_map(|(key, value)| match value {
            toml::Value::Table(inner) => {
                dotted_keys(inner).into_iter().map(|k| format!("{key}.{k}")).collect()
            }
            _ => BTreeSet::from([key.clone()]),
        })
        .collect()
}

pub fn read(path: &Path) -> anyhow::Result<toml::Table> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text).with_context(|| format!("in {}", path.display()))
}

pub fn run_settings(
    file: Option<toml::Table>,
    sets: &[String],
    url: Option<String>,
) -> anyhow::Result<Settings> {
    let mut table = file.unwrap_or_default();
    for arg in sets {
        merge(&mut table, set(arg)?);
    }
    if let Some(url) = url {
        table.insert("url".into(), url.into());
    }
    let settings: Settings = table.try_into()?;
    settings.validate()?;
    Ok(settings)
}

pub fn rejudge(
    settings: &mut Settings,
    file: Option<toml::Table>,
    sets: &[String],
) -> anyhow::Result<()> {
    let mut overrides = file.unwrap_or_default();
    overrides.retain(|key, _| JUDGING.contains(&key));
    for arg in sets {
        let table = set(arg)?;
        anyhow::ensure!(
            table.keys().all(|key| JUDGING.contains(&key.as_str())),
            "--set {arg}: a report is re-judged only with slo.*, limits.* and scoring.* keys"
        );
        merge(&mut overrides, table);
    }
    let mut table = toml::Table::try_from(&*settings)?;
    merge(&mut table, overrides);
    *settings = table.try_into()?;
    settings.validate()
}

fn set(arg: &str) -> anyhow::Result<toml::Table> {
    let (key, value) =
        arg.split_once('=').with_context(|| format!("--set {arg}: expected KEY=VALUE"))?;
    let bare = !value.is_empty()
        && value.chars().all(|c| c.is_ascii_alphanumeric() || "_-.:/".contains(c));
    let text = match toml::from_str::<toml::Table>(arg) {
        Err(_) if bare => format!("{key} = {}", toml::Value::from(value)),
        _ => arg.to_string(),
    };
    parse(&text).with_context(|| format!("--set {arg}"))
}

fn merge(into: &mut toml::Table, from: toml::Table) {
    for (key, value) in from {
        match (into.get_mut(&key), value) {
            (Some(toml::Value::Table(inner)), toml::Value::Table(value)) => merge(inner, value),
            (_, value) => {
                into.insert(key, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_lists_every_key_at_its_default() {
        let text = include_str!("../load-test.example.toml");
        let example = parse(text).expect("example parses");
        let mut defaults = toml::Table::try_from(Settings::default()).expect("serializes");
        defaults.remove("url");
        defaults.remove("workers");
        assert_eq!(dotted_keys(&example), dotted_keys(&defaults));
        assert_eq!(example.try_into::<Settings>().expect("typed"), Settings::default());

        let uncommented: Vec<&str> = text
            .lines()
            .map(|line| match line.strip_prefix("# ") {
                Some(rest) if rest.split_once(" = ").is_some_and(|(key, _)| is_key(key)) => rest,
                _ => line,
            })
            .collect();
        parse(&uncommented.join("\n")).expect("every commented-out key is a real key");
    }

    fn is_key(word: &str) -> bool {
        word.chars().all(|c| c.is_ascii_lowercase() || c == '_')
    }
}
