//! Gold+ is two u.gg buckets (Gold and Platinum+): their count columns are
//! summed before any rate is computed.

use super::*;

/// Merge only the count fields we consume; unrelated derived upstream fields
/// (rankings, deltas, timestamps, etc.) must never be summed or averaged.
fn merge_count_rows(
    left: &[Value],
    right: &[Value],
    ids: &[usize],
    counts: &[usize],
) -> Option<Vec<Value>> {
    let width = ids.iter().chain(counts).max()? + 1;
    let mut merged = BTreeMap::<Vec<String>, Vec<Value>>::new();
    for row in left.iter().chain(right) {
        let key: Vec<String> = ids
            .iter()
            .map(|index| {
                let id = row.get(*index)?;
                let id = id
                    .as_str()
                    .and_then(|text| text.parse::<u64>().ok())
                    .or_else(|| id.as_u64())?;
                (id > 0).then(|| id.to_string())
            })
            .collect::<Option<_>>()?;
        let entry = merged.entry(key).or_insert_with(|| {
            let mut entry = vec![Value::Null; width];
            for index in ids {
                entry[*index] = row[*index].clone();
            }
            for index in counts {
                entry[*index] = Value::from(0u64);
            }
            entry
        });
        for index in counts {
            entry[*index] = Value::from(
                entry[*index]
                    .as_u64()?
                    .checked_add(row.get(*index)?.as_u64()?)?,
            );
        }
    }
    Some(merged.into_values().map(Value::Array).collect())
}

fn array_or_empty(value: Option<&Value>) -> Option<&[Value]> {
    match value {
        Some(value) => Some(value.as_array()?.as_slice()),
        None => Some(&[]),
    }
}

pub(super) fn merge_datasets(kind: &str, left: &Value, right: &Value) -> Option<Value> {
    match kind {
        "champion_ranking" => {
            let (left_roles, right_roles) = (left.get(0)?.as_object()?, right.get(0)?.as_object()?);
            let mut roles = serde_json::Map::new();
            for role in left_roles
                .keys()
                .chain(right_roles.keys())
                .collect::<BTreeSet<_>>()
            {
                let rows = merge_count_rows(
                    array_or_empty(left_roles.get(role))?,
                    array_or_empty(right_roles.get(role))?,
                    &[0],
                    &[2, 3],
                )?;
                roles.insert(role.clone(), Value::Array(rows));
            }
            let (left_bans, right_bans) = (left.get(1)?.as_object()?, right.get(1)?.as_object()?);
            let mut bans = serde_json::Map::new();
            for id in left_bans
                .keys()
                .chain(right_bans.keys())
                .collect::<BTreeSet<_>>()
            {
                let count = |map: &serde_json::Map<String, Value>| match map.get(id) {
                    Some(value) => value.as_u64(),
                    None => Some(0),
                };
                bans.insert(
                    id.clone(),
                    Value::from(count(left_bans)?.checked_add(count(right_bans)?)?),
                );
            }
            Some(serde_json::json!([
                roles,
                bans,
                null,
                left.get(3)?.as_f64()? + right.get(3)?.as_f64()?
            ]))
        }
        "matchups" => {
            let (left, right) = (left.as_object()?, right.as_object()?);
            let mut roles = serde_json::Map::new();
            for role in left.keys().chain(right.keys()).collect::<BTreeSet<_>>() {
                let rows = |map: &serde_json::Map<String, Value>| match map.get(role) {
                    Some(value) => Some(value.get(0)?.as_array()?.clone()),
                    None => Some(Vec::new()),
                };
                roles.insert(
                    role.clone(),
                    serde_json::json!([
                        merge_count_rows(&rows(left)?, &rows(right)?, &[0], &[1, 2])?,
                        null
                    ]),
                );
            }
            Some(Value::Object(roles))
        }
        "duos" => Some(serde_json::json!([{"adc_supp":merge_count_rows(
            left.get(0)?.get("adc_supp")?.as_array()?, right.get(0)?.get("adc_supp")?.as_array()?,
            &[0,1,4,5], &[2,3,6,7,8,9])?}])),
        _ => None,
    }
}
