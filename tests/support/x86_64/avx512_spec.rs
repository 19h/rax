//! Shared EVEX SIMD specification-row parser for x86_64 tests.

use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EvexVl {
    Vl128,
    Vl256,
    Vl512,
    LlIg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EvexW {
    W0,
    W1,
    WIg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EvexOperandForm {
    RegisterOnly,
    RegisterOrMemory,
    MemoryOnly,
    VsibMemory,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EvexEncodingKey {
    pub mnemonic: String,
    pub map: u8,
    pub opcode: u8,
    pub pp: u8,
    pub w: EvexW,
    pub vl: EvexVl,
    pub opcode_ext: Option<u8>,
    pub form: EvexOperandForm,
    pub imm: bool,
}

#[derive(Clone, Debug)]
pub struct EvexSpecRow {
    pub source: String,
    pub cell: String,
    pub operands: String,
    pub key: EvexEncodingKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EvexAsmMode {
    Register,
    Memory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EvexCaseVariant {
    pub mode: EvexAsmMode,
    pub rm_reg: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum EvexRmRegisterClass {
    Vector,
    Gpr,
    Mask,
    Unknown,
}

fn ascii_words(text: &str) -> Vec<(usize, usize, &str)> {
    let mut words = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if ch.is_ascii_alphanumeric() {
            start.get_or_insert(index);
        } else if let Some(word_start) = start.take() {
            words.push((word_start, index, &text[word_start..index]));
        }
    }
    if let Some(word_start) = start {
        words.push((word_start, text.len(), &text[word_start..]));
    }
    words
}

pub fn evex_cell_parts(cell: &str) -> Option<(String, String, String)> {
    let normalized = cell
        .replace("ibV", "ib V")
        .replace("iwV", "iw V")
        .replace("  ", " ");

    for (start, end, token) in ascii_words(&normalized) {
        let mut chars = token.chars();
        let starts_like_vector_mnemonic = chars.next() == Some('V')
            && token.len() > 2
            && token.chars().all(|ch| ch.is_ascii_alphanumeric());
        if starts_like_vector_mnemonic && token != "VEX" {
            return Some((
                normalized[..start].trim().to_string(),
                token.to_ascii_lowercase(),
                normalized[end..].trim().to_string(),
            ));
        }
    }

    None
}

fn evex_prefix_tokens(prefix: &str) -> Vec<String> {
    prefix
        .replace('.', " ")
        .split_whitespace()
        .map(|token| token.trim().to_ascii_uppercase())
        .collect()
}

fn parse_evex_vl(tokens: &[String]) -> Option<EvexVl> {
    if tokens.iter().any(|token| token == "128") {
        Some(EvexVl::Vl128)
    } else if tokens.iter().any(|token| token == "256") {
        Some(EvexVl::Vl256)
    } else if tokens.iter().any(|token| token == "512") {
        Some(EvexVl::Vl512)
    } else if tokens.iter().any(|token| token == "LLIG") {
        Some(EvexVl::LlIg)
    } else {
        None
    }
}

fn parse_evex_pp(tokens: &[String]) -> u8 {
    let w_index = tokens
        .iter()
        .position(|token| token.starts_with('W'))
        .unwrap_or(tokens.len());
    let before_w = &tokens[..w_index];
    if before_w.iter().any(|token| token == "66") {
        1
    } else if before_w.iter().any(|token| token == "F3") {
        2
    } else if before_w.iter().any(|token| token == "F2") {
        3
    } else {
        0
    }
}

fn parse_evex_map(tokens: &[String]) -> Option<u8> {
    if tokens.iter().any(|token| token == "MAP5") {
        Some(5)
    } else if tokens.iter().any(|token| token == "MAP6") {
        Some(6)
    } else if tokens.iter().any(|token| token == "0F3A") {
        Some(3)
    } else if tokens.iter().any(|token| token == "0F38") {
        Some(2)
    } else if tokens.iter().any(|token| token == "0F") {
        Some(1)
    } else {
        None
    }
}

fn parse_evex_w(tokens: &[String]) -> EvexW {
    tokens
        .iter()
        .find_map(|token| match token.as_str() {
            "W0" => Some(EvexW::W0),
            "W1" => Some(EvexW::W1),
            "WIG" => Some(EvexW::WIg),
            _ => None,
        })
        .unwrap_or(EvexW::WIg)
}

fn hex_byte_prefix(token: &str) -> Option<u8> {
    let mut chars = token.chars();
    let high = chars.next()?;
    let low = chars.next()?;
    if !high.is_ascii_hexdigit() || !low.is_ascii_hexdigit() {
        return None;
    }
    if chars.next().is_some_and(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    u8::from_str_radix(&token[..2], 16).ok()
}

fn is_evex_metadata_hex(token: &str) -> bool {
    matches!(token, "0F" | "0F38" | "0F3A" | "66" | "F2" | "F3")
}

fn parse_evex_opcode(tokens: &[String]) -> Option<u8> {
    if let Some(w_index) = tokens.iter().position(|token| token.starts_with('W')) {
        if let Some(opcode) = tokens[w_index + 1..]
            .iter()
            .find_map(|token| hex_byte_prefix(token))
        {
            return Some(opcode);
        }
    }

    tokens.iter().rev().find_map(|token| {
        if is_evex_metadata_hex(token) {
            return None;
        }
        hex_byte_prefix(token)
    })
}

fn parse_evex_opcode_ext(tokens: &[String]) -> Option<u8> {
    tokens.iter().find_map(|token| {
        let token = token.strip_prefix('/')?;
        if token.len() == 1 {
            token.parse::<u8>().ok().filter(|value| *value < 8)
        } else {
            None
        }
    })
}

fn parse_evex_form(prefix: &str, operands: &str) -> EvexOperandForm {
    let lower_prefix = prefix.to_ascii_lowercase();
    let lower_operands = operands.to_ascii_lowercase();
    if lower_prefix.contains("/vsib")
        || lower_operands.contains("vm32")
        || lower_operands.contains("vm64")
    {
        EvexOperandForm::VsibMemory
    } else if lower_operands.contains("/m") {
        EvexOperandForm::RegisterOrMemory
    } else if lower_operands
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '/'))
        .any(|token| token.starts_with('m') && token[1..].chars().all(|ch| ch.is_ascii_digit()))
    {
        EvexOperandForm::MemoryOnly
    } else {
        EvexOperandForm::RegisterOnly
    }
}

fn parse_evex_imm(prefix: &str, operands: &str) -> bool {
    let lower_prefix = prefix.to_ascii_lowercase();
    let lower_operands = operands.to_ascii_lowercase();
    lower_prefix
        .split_whitespace()
        .any(|token| matches!(token, "ib" | "iw"))
        || lower_operands.contains("imm")
}

fn parse_evex_spec_row(source: String, cell: &str) -> Result<EvexSpecRow, String> {
    let (prefix, mnemonic, operands) =
        evex_cell_parts(cell).ok_or_else(|| format!("{source}: missing EVEX mnemonic: {cell}"))?;
    let tokens = evex_prefix_tokens(&prefix);
    let key = EvexEncodingKey {
        mnemonic,
        map: parse_evex_map(&tokens)
            .ok_or_else(|| format!("{source}: missing EVEX map: {cell}"))?,
        opcode: parse_evex_opcode(&tokens)
            .ok_or_else(|| format!("{source}: missing EVEX opcode: {cell}"))?,
        pp: parse_evex_pp(&tokens),
        w: parse_evex_w(&tokens),
        vl: parse_evex_vl(&tokens)
            .ok_or_else(|| format!("{source}: missing EVEX vector length: {cell}"))?,
        opcode_ext: parse_evex_opcode_ext(&tokens),
        form: parse_evex_form(&prefix, &operands),
        imm: parse_evex_imm(&prefix, &operands),
    };

    Ok(EvexSpecRow {
        source,
        cell: cell.to_string(),
        operands,
        key,
    })
}

pub fn evex_operand_parts(operands: &str) -> Vec<String> {
    operands
        .split(',')
        .map(str::trim)
        .filter(|operand| !operand.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn evex_strip_operand_decorations(operand: &str) -> String {
    let mut stripped = String::new();
    let mut in_braces = false;

    for ch in operand.chars() {
        match ch {
            '{' => in_braces = true,
            '}' => in_braces = false,
            _ if !in_braces => stripped.push(ch),
            _ => {}
        }
    }

    stripped.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn evex_normalize_operand(operand: &str) -> String {
    evex_strip_operand_decorations(operand).to_ascii_lowercase()
}

pub fn evex_operand_is_immediate(operand: &str) -> bool {
    let lower = evex_normalize_operand(operand);
    lower.starts_with("imm") || lower == "ib" || lower == "iw" || lower.contains("imm")
}

fn operand_has_vector_register(operand: &str) -> bool {
    let lower = evex_normalize_operand(operand);
    lower.contains("xmm") || lower.contains("ymm") || lower.contains("zmm")
}

fn operand_has_memory_alternative(operand: &str) -> bool {
    let lower = evex_normalize_operand(operand);
    lower.contains("/m")
        || lower.contains("vm32")
        || lower.contains("vm64")
        || lower
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '/'))
            .any(|token| token.starts_with('m') && token[1..].chars().all(|ch| ch.is_ascii_digit()))
}

pub fn evex_rm_operand_index(row: &EvexSpecRow) -> Option<usize> {
    let operands = evex_operand_parts(&row.operands);
    operands
        .iter()
        .position(|operand| operand_has_memory_alternative(operand))
        .or_else(|| {
            (row.key.form == EvexOperandForm::RegisterOnly).then(|| {
                operands
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, operand)| !evex_operand_is_immediate(operand))
                    .map(|(index, _)| index)
                    .unwrap_or(0)
            })
        })
}

pub fn evex_rm_register_class(row: &EvexSpecRow) -> EvexRmRegisterClass {
    let operands = evex_operand_parts(&row.operands);
    let Some(index) = evex_rm_operand_index(row) else {
        return EvexRmRegisterClass::Unknown;
    };
    let Some(operand) = operands.get(index) else {
        return EvexRmRegisterClass::Unknown;
    };
    let lower = evex_normalize_operand(operand);

    if lower.contains("xmm") || lower.contains("ymm") || lower.contains("zmm") {
        EvexRmRegisterClass::Vector
    } else if lower == "reg"
        || lower.starts_with('r')
        || lower.contains("r/m")
        || lower.contains("reg/m")
        || lower.starts_with("r8/")
        || lower.starts_with("r16/")
        || lower.starts_with("r32/")
        || lower.starts_with("r64/")
    {
        EvexRmRegisterClass::Gpr
    } else if lower.starts_with('k') || (lower.contains('k') && lower.contains("/m")) {
        EvexRmRegisterClass::Mask
    } else {
        EvexRmRegisterClass::Unknown
    }
}

pub fn evex_rm_register_buckets_for_row(row: &EvexSpecRow) -> Vec<u8> {
    match evex_rm_register_class(row) {
        EvexRmRegisterClass::Vector => vec![0, 8, 16, 24],
        EvexRmRegisterClass::Gpr => vec![0, 8],
        EvexRmRegisterClass::Mask => vec![0],
        EvexRmRegisterClass::Unknown => vec![0],
    }
}

pub fn evex_vvvv_value_for_row(row: &EvexSpecRow) -> u8 {
    let operands = evex_operand_parts(&row.operands)
        .into_iter()
        .filter(|operand| !evex_operand_is_immediate(operand))
        .collect::<Vec<_>>();

    if operands.len() >= 3
        && operands
            .get(1)
            .is_some_and(|operand| operand_has_vector_register(operand))
    {
        0
    } else {
        15
    }
}

pub fn avx512_spec_evex_rows() -> Vec<EvexSpecRow> {
    let spec_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/specifications/x86_64");
    let mut rows = Vec::new();
    let mut failures = Vec::new();

    for entry in fs::read_dir(&spec_dir).expect("x86_64 spec directory must exist") {
        let entry = entry.expect("spec directory entry must be readable");
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("txt") {
            continue;
        }

        let text = fs::read_to_string(entry.path()).expect("x86_64 spec file must be readable");
        for (line_index, line) in text.lines().enumerate() {
            if !line.contains('|') || !line.contains("EVEX") {
                continue;
            }

            for cell in line.split('|').map(str::trim) {
                let cell = if cell.starts_with("EVEX") {
                    cell
                } else if let Some(evex_start) = cell.find("EVEX.") {
                    let candidate = &cell[evex_start..];
                    if !candidate.contains("MAP") {
                        continue;
                    }
                    candidate
                } else {
                    continue;
                };
                if evex_cell_parts(cell).is_none() {
                    continue;
                }

                let source = format!(
                    "{}:{}",
                    entry.path().file_name().unwrap().to_string_lossy(),
                    line_index + 1
                );
                match parse_evex_spec_row(source, cell) {
                    Ok(row) => rows.push(row),
                    Err(failure) => failures.push(failure),
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "failed to parse AVX-512 EVEX encoding rows:\n{}",
        failures.join("\n")
    );
    rows
}

pub fn evex_asm_modes_for_row(row: &EvexSpecRow) -> Vec<EvexAsmMode> {
    match row.key.form {
        EvexOperandForm::RegisterOnly => vec![EvexAsmMode::Register],
        EvexOperandForm::RegisterOrMemory => vec![EvexAsmMode::Register, EvexAsmMode::Memory],
        EvexOperandForm::MemoryOnly | EvexOperandForm::VsibMemory => vec![EvexAsmMode::Memory],
    }
}

pub fn evex_case_variants_for_row(row: &EvexSpecRow) -> Vec<EvexCaseVariant> {
    evex_asm_modes_for_row(row)
        .into_iter()
        .flat_map(|mode| match mode {
            EvexAsmMode::Register => evex_rm_register_buckets_for_row(row)
                .into_iter()
                .map(move |rm_reg| EvexCaseVariant {
                    mode,
                    rm_reg: Some(rm_reg),
                })
                .collect::<Vec<_>>(),
            EvexAsmMode::Memory => vec![EvexCaseVariant { mode, rm_reg: None }],
        })
        .collect()
}

pub fn evex_vl_bits(vl: EvexVl) -> u8 {
    match vl {
        EvexVl::Vl128 | EvexVl::LlIg => 0,
        EvexVl::Vl256 => 1,
        EvexVl::Vl512 => 2,
    }
}

pub fn raw_evex_spec_bytes_for_variant(row: &EvexSpecRow, variant: EvexCaseVariant) -> Vec<u8> {
    let p0 = 0xf0 | row.key.map;
    let mut p0 = p0;
    if let Some(rm_reg) = variant.rm_reg {
        assert!(rm_reg < 32, "EVEX r/m register index must be 0..31");
        if (rm_reg & 0x08) != 0 {
            p0 &= !0x20;
        }
        if (rm_reg & 0x10) != 0 {
            p0 &= !0x40;
        }
    }

    let vvvv = evex_vvvv_value_for_row(row);
    let p1 = (((!vvvv) & 0x0f) << 3)
        | row.key.pp
        | match row.key.w {
            EvexW::W1 => 0x80,
            EvexW::W0 | EvexW::WIg => 0,
        };
    let mask = if row.cell.contains("{k") { 1 } else { 0 };
    let p2 = (evex_vl_bits(row.key.vl) << 5) | mask;
    let reg_field = row.key.opcode_ext.unwrap_or(1);
    let modrm = match variant.mode {
        EvexAsmMode::Register => 0xc0 | (reg_field << 3) | (variant.rm_reg.unwrap_or(0) & 0x7),
        EvexAsmMode::Memory if row.key.form == EvexOperandForm::VsibMemory => (reg_field << 3) | 4,
        EvexAsmMode::Memory => reg_field << 3,
    };

    let mut bytes = vec![0x62, p0, p1, p2, row.key.opcode, modrm];
    if variant.mode == EvexAsmMode::Memory && row.key.form == EvexOperandForm::VsibMemory {
        bytes.push(0x80);
    }
    if row.key.imm {
        bytes.push(3);
    }
    bytes
}

pub fn spec_row_mode_id(row: &EvexSpecRow, mode: EvexAsmMode) -> String {
    format!("{} {:?} {}", row.source, mode, row.cell)
}

pub fn spec_case_variant_id(row: &EvexSpecRow, variant: EvexCaseVariant) -> String {
    match variant.rm_reg {
        Some(rm_reg) => format!("{} {:?} rm{rm_reg} {}", row.source, variant.mode, row.cell),
        None => spec_row_mode_id(row, variant.mode),
    }
}
