use crate::TwlawResult;
use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::OnceLock;

const PCODE_ALL_JSON_GZ: &[u8] = include_bytes!("../data/pcode_all.json.gz");
const LAW_HISTORIES_JSON_GZ: &[u8] = include_bytes!("../data/law_histories.json.gz");
const OLD_CASES_JSON_GZ: &[u8] = include_bytes!("../data/old_cases.json.gz");
const NEW_CASES_JSON_GZ: &[u8] = include_bytes!("../data/new_cases.json.gz");

#[derive(Debug, Deserialize)]
struct PcodeRaw {
    pcode_map: HashMap<String, String>,
    #[serde(default)]
    abolished_set: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PcodeMatch {
    pub law_name: String,
    pub pcode: String,
    pub status: String,
    pub match_type: String,
}

#[derive(Debug)]
pub struct PcodeData {
    pub pcode_map: HashMap<String, String>,
    pub reverse: HashMap<String, String>,
    pub abolished: HashSet<String>,
    pub histories: HashMap<String, String>,
}

static PCODE_DATA: OnceLock<PcodeData> = OnceLock::new();
static OLD_CASES: OnceLock<HashMap<String, Value>> = OnceLock::new();
static NEW_CASES: OnceLock<HashMap<String, Value>> = OnceLock::new();

const LAW_ALIASES: &[(&str, &str)] = &[
    ("消保法", "消費者保護法"),
    ("勞基法", "勞動基準法"),
    ("個資法", "個人資料保護法"),
    ("國賠法", "國家賠償法"),
    ("道交條例", "道路交通管理處罰條例"),
    ("證交法", "證券交易法"),
    ("公交法", "公平交易法"),
    ("強執法", "強制執行法"),
    ("家事法", "家事事件法"),
    ("少事法", "少年事件處理法"),
    ("社維法", "社會秩序維護法"),
    ("行程法", "行政程序法"),
    ("民訴法", "民事訴訟法"),
    ("刑訴法", "刑事訴訟法"),
    ("行訴法", "行政訴訟法"),
    ("不經條例", "不動產經紀業管理條例"),
    ("智財法", "智慧財產案件審理法"),
    ("稅徵法", "稅捐稽徵法"),
    ("政採法", "政府採購法"),
    ("遺贈稅法", "遺產及贈與稅法"),
    ("公寓條例", "公寓大廈管理條例"),
    ("大廈條例", "公寓大廈管理條例"),
    ("營業稅法", "加值型及非加值型營業稅法"),
    ("刑法", "中華民國刑法"),
];

pub fn pcode_data() -> TwlawResult<&'static PcodeData> {
    Ok(PCODE_DATA.get_or_init(|| {
        let pcode_json = decompress_gzip(PCODE_ALL_JSON_GZ).expect("bundled pcode gzip is valid");
        let history_json =
            decompress_gzip(LAW_HISTORIES_JSON_GZ).expect("bundled law history gzip is valid");
        let raw: PcodeRaw = serde_json::from_str(&pcode_json).expect("bundled pcode JSON is valid");
        let histories: HashMap<String, String> =
            serde_json::from_str(&history_json).expect("bundled law history JSON is valid");
        let reverse = raw
            .pcode_map
            .iter()
            .map(|(name, pcode)| (pcode.clone(), name.clone()))
            .collect();
        PcodeData {
            pcode_map: raw.pcode_map,
            reverse,
            abolished: raw.abolished_set.into_iter().collect(),
            histories,
        }
    }))
}

pub fn law_status(pcode: &str) -> TwlawResult<String> {
    let data = pcode_data()?;
    if data.abolished.contains(pcode) {
        Ok("已廢止".to_string())
    } else {
        Ok("現行法規".to_string())
    }
}

pub fn resolve_pcode(law_name: &str) -> TwlawResult<Option<PcodeMatch>> {
    let name = law_name.trim();
    if name.is_empty() {
        return Ok(None);
    }

    let data = pcode_data()?;
    if let Some(pcode) = data.pcode_map.get(name) {
        return Ok(Some(PcodeMatch {
            law_name: name.to_string(),
            pcode: pcode.clone(),
            status: law_status(pcode)?,
            match_type: "exact".to_string(),
        }));
    }

    if let Some((_, full_name)) = LAW_ALIASES.iter().find(|(alias, _)| *alias == name) {
        if let Some(pcode) = data.pcode_map.get(*full_name) {
            return Ok(Some(PcodeMatch {
                law_name: (*full_name).to_string(),
                pcode: pcode.clone(),
                status: law_status(pcode)?,
                match_type: "alias".to_string(),
            }));
        }
    }

    let mut candidates: Vec<(&String, &String)> = data
        .pcode_map
        .iter()
        .filter(|(candidate, _)| candidate.contains(name) || name.contains(candidate.as_str()))
        .collect();

    if candidates.is_empty() {
        return Ok(None);
    }

    candidates.sort_by(|(a, _), (b, _)| {
        let a_contains = a.contains(name);
        let b_contains = b.contains(name);
        match (a_contains, b_contains) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.len().cmp(&b.len()).then_with(|| a.cmp(b)),
        }
    });

    let (matched_name, pcode) = candidates[0];
    Ok(Some(PcodeMatch {
        law_name: matched_name.clone(),
        pcode: pcode.clone(),
        status: law_status(pcode)?,
        match_type: "fuzzy".to_string(),
    }))
}

pub fn law_name_for_pcode(pcode: &str) -> TwlawResult<String> {
    let data = pcode_data()?;
    Ok(data.reverse.get(pcode).cloned().unwrap_or_default())
}

pub fn law_history(pcode: &str) -> TwlawResult<Option<String>> {
    Ok(pcode_data()?.histories.get(pcode).cloned())
}

pub fn old_cases() -> TwlawResult<&'static HashMap<String, Value>> {
    Ok(OLD_CASES.get_or_init(|| {
        let json = decompress_gzip(OLD_CASES_JSON_GZ).expect("bundled old cases gzip is valid");
        serde_json::from_str(&json).expect("bundled old cases JSON is valid")
    }))
}

pub fn new_cases() -> TwlawResult<&'static HashMap<String, Value>> {
    Ok(NEW_CASES.get_or_init(|| {
        let json = decompress_gzip(NEW_CASES_JSON_GZ).expect("bundled new cases gzip is valid");
        serde_json::from_str(&json).expect("bundled new cases JSON is valid")
    }))
}

fn decompress_gzip(bytes: &[u8]) -> std::io::Result<String> {
    let mut decoder = GzDecoder::new(bytes);
    let mut out = String::new();
    decoder.read_to_string(&mut out)?;
    Ok(out)
}

pub fn value_str<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

pub fn value_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}
