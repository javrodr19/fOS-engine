//! Date
//!
//! Time values are milliseconds since the epoch, computed with the
//! specification's day/year arithmetic. The local time zone is UTC (no
//! time zone database yet), so local and UTC methods agree.

use crate::gc::Gc;
use crate::object::*;
use crate::value::Value;
use crate::vm::ops::Hint;
use crate::vm::{JsResult, NativeFn, Vm};

use super::arg;

const MS_PER_DAY: f64 = 86_400_000.0;
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

pub(super) fn init(vm: &mut Vm) {
    let proto = vm.realm.date_proto;
    let c = vm.def_ctor("Date", 7, date_call, Some(date_construct), proto);
    vm.def_method(c, "now", 0, now);
    vm.def_method(c, "parse", 1, parse);
    vm.def_method(c, "UTC", 7, utc);

    let getters: &[(&str, NativeFn)] = &[
        ("getTime", get_time),
        ("valueOf", get_time),
        ("getFullYear", |vm, t, _, _| field(vm, t, year_from_time)),
        ("getUTCFullYear", |vm, t, _, _| field(vm, t, year_from_time)),
        ("getMonth", |vm, t, _, _| field(vm, t, month_from_time)),
        ("getUTCMonth", |vm, t, _, _| field(vm, t, month_from_time)),
        ("getDate", |vm, t, _, _| field(vm, t, date_from_time)),
        ("getUTCDate", |vm, t, _, _| field(vm, t, date_from_time)),
        ("getDay", |vm, t, _, _| field(vm, t, week_day)),
        ("getUTCDay", |vm, t, _, _| field(vm, t, week_day)),
        ("getHours", |vm, t, _, _| field(vm, t, hour_from_time)),
        ("getUTCHours", |vm, t, _, _| field(vm, t, hour_from_time)),
        ("getMinutes", |vm, t, _, _| field(vm, t, min_from_time)),
        ("getUTCMinutes", |vm, t, _, _| field(vm, t, min_from_time)),
        ("getSeconds", |vm, t, _, _| field(vm, t, sec_from_time)),
        ("getUTCSeconds", |vm, t, _, _| field(vm, t, sec_from_time)),
        ("getMilliseconds", |vm, t, _, _| field(vm, t, ms_from_time)),
        ("getUTCMilliseconds", |vm, t, _, _| field(vm, t, ms_from_time)),
        ("getYear", |vm, t, _, _| field(vm, t, |t| year_from_time(t) - 1900.0)),
        ("getTimezoneOffset", |vm, t, _, _| field(vm, t, |_| 0.0)),
    ];
    for &(name, f) in getters {
        vm.def_method(proto, name, 0, f);
    }
    let setters: &[(&str, u32, NativeFn)] = &[
        ("setTime", 1, set_time),
        ("setMilliseconds", 1, |vm, t, a, _| set_fields(vm, t, a, 6)),
        ("setUTCMilliseconds", 1, |vm, t, a, _| set_fields(vm, t, a, 6)),
        ("setSeconds", 2, |vm, t, a, _| set_fields(vm, t, a, 5)),
        ("setUTCSeconds", 2, |vm, t, a, _| set_fields(vm, t, a, 5)),
        ("setMinutes", 3, |vm, t, a, _| set_fields(vm, t, a, 4)),
        ("setUTCMinutes", 3, |vm, t, a, _| set_fields(vm, t, a, 4)),
        ("setHours", 4, |vm, t, a, _| set_fields(vm, t, a, 3)),
        ("setUTCHours", 4, |vm, t, a, _| set_fields(vm, t, a, 3)),
        ("setDate", 1, |vm, t, a, _| set_fields(vm, t, a, 2)),
        ("setUTCDate", 1, |vm, t, a, _| set_fields(vm, t, a, 2)),
        ("setMonth", 2, |vm, t, a, _| set_fields(vm, t, a, 1)),
        ("setUTCMonth", 2, |vm, t, a, _| set_fields(vm, t, a, 1)),
        ("setFullYear", 3, |vm, t, a, _| set_fields(vm, t, a, 0)),
        ("setUTCFullYear", 3, |vm, t, a, _| set_fields(vm, t, a, 0)),
    ];
    for &(name, len, f) in setters {
        vm.def_method(proto, name, len, f);
    }
    let formats: &[(&str, NativeFn)] = &[
        ("toISOString", to_iso_string),
        ("toJSON", to_json),
        ("toString", to_string),
        ("toDateString", to_date_string),
        ("toTimeString", to_time_string),
        ("toUTCString", to_utc_string),
        ("toGMTString", to_utc_string),
        ("toLocaleString", to_locale_string),
        ("toLocaleDateString", to_locale_date_string),
        ("toLocaleTimeString", to_locale_time_string),
    ];
    for &(name, f) in formats {
        vm.def_method(proto, name, 0, f);
    }
    let tp = vm.sym.to_primitive;
    vm.def_method_sym(proto, tp, "[Symbol.toPrimitive]", 1, to_primitive);
}

// ---- time arithmetic (ECMAScript 21.4.1) ----

fn day(t: f64) -> f64 {
    (t / MS_PER_DAY).floor()
}

fn time_within_day(t: f64) -> f64 {
    t.rem_euclid(MS_PER_DAY)
}

fn days_in_year(y: f64) -> f64 {
    if y % 4.0 != 0.0 {
        365.0
    } else if y % 100.0 != 0.0 {
        366.0
    } else if y % 400.0 != 0.0 {
        365.0
    } else {
        366.0
    }
}

fn day_from_year(y: f64) -> f64 {
    365.0 * (y - 1970.0) + ((y - 1969.0) / 4.0).floor() - ((y - 1901.0) / 100.0).floor() + ((y - 1601.0) / 400.0).floor()
}

fn time_from_year(y: f64) -> f64 {
    MS_PER_DAY * day_from_year(y)
}

fn year_from_time(t: f64) -> f64 {
    let mut y = (t / (MS_PER_DAY * 365.2425)).floor() + 1970.0;
    while time_from_year(y) > t {
        y -= 1.0;
    }
    while time_from_year(y + 1.0) <= t {
        y += 1.0;
    }
    y
}

fn in_leap_year(t: f64) -> bool {
    days_in_year(year_from_time(t)) == 366.0
}

fn day_within_year(t: f64) -> f64 {
    day(t) - day_from_year(year_from_time(t))
}

/// Cumulative days before each month (non-leap)
const MONTH_STARTS: [f64; 13] = [0.0, 31.0, 59.0, 90.0, 120.0, 151.0, 181.0, 212.0, 243.0, 273.0, 304.0, 334.0, 365.0];

fn month_start(m: usize, leap: bool) -> f64 {
    MONTH_STARTS[m] + if leap && m >= 2 { 1.0 } else { 0.0 }
}

fn month_from_time(t: f64) -> f64 {
    let d = day_within_year(t);
    let leap = in_leap_year(t);
    (0..12).find(|&m| d < month_start(m + 1, leap)).unwrap_or(11) as f64
}

fn date_from_time(t: f64) -> f64 {
    let d = day_within_year(t);
    let m = month_from_time(t) as usize;
    d - month_start(m, in_leap_year(t)) + 1.0
}

fn week_day(t: f64) -> f64 {
    (day(t) + 4.0).rem_euclid(7.0)
}

fn hour_from_time(t: f64) -> f64 {
    (time_within_day(t) / 3_600_000.0).floor()
}

fn min_from_time(t: f64) -> f64 {
    (time_within_day(t) / 60_000.0).floor() % 60.0
}

fn sec_from_time(t: f64) -> f64 {
    (time_within_day(t) / 1000.0).floor() % 60.0
}

fn ms_from_time(t: f64) -> f64 {
    time_within_day(t) % 1000.0
}

fn make_time(h: f64, m: f64, s: f64, ms: f64) -> f64 {
    if !(h.is_finite() && m.is_finite() && s.is_finite() && ms.is_finite()) {
        return f64::NAN;
    }
    h.trunc() * 3_600_000.0 + m.trunc() * 60_000.0 + s.trunc() * 1000.0 + ms.trunc()
}

fn make_day(year: f64, month: f64, date: f64) -> f64 {
    if !(year.is_finite() && month.is_finite() && date.is_finite()) {
        return f64::NAN;
    }
    let (y, m, dt) = (year.trunc(), month.trunc(), date.trunc());
    let ym = y + (m / 12.0).floor();
    if ym.abs() > 400_000.0 {
        return f64::NAN;
    }
    let mn = m.rem_euclid(12.0) as usize;
    day_from_year(ym) + month_start(mn, days_in_year(ym) == 366.0) + dt - 1.0
}

fn make_date(day: f64, time: f64) -> f64 {
    if !(day.is_finite() && time.is_finite()) {
        return f64::NAN;
    }
    day * MS_PER_DAY + time
}

fn time_clip(t: f64) -> f64 {
    if !t.is_finite() || t.abs() > 8.64e15 {
        return f64::NAN;
    }
    t.trunc() + 0.0
}

fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

// ---- parsing ----

/// Parse the ISO format and the common human-readable formats
pub(crate) fn parse_date(s: &str) -> f64 {
    let s = s.trim();
    if let Some(t) = parse_iso(s) {
        return t;
    }
    parse_loose(s).unwrap_or(f64::NAN)
}

fn parse_iso(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    let mut i = 0;
    let num = |i: &mut usize, n: usize| -> Option<f64> {
        let part = s.get(*i..*i + n)?;
        if !part.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        *i += n;
        part.parse().ok()
    };
    let year = if b.first() == Some(&b'+') || b.first() == Some(&b'-') {
        let neg = b[0] == b'-';
        i = 1;
        let y = num(&mut i, 6)?;
        if neg { -y } else { y }
    } else {
        num(&mut i, 4)?
    };
    let (mut month, mut dayn) = (1.0, 1.0);
    if b.get(i) == Some(&b'-') {
        i += 1;
        month = num(&mut i, 2)?;
        if b.get(i) == Some(&b'-') {
            i += 1;
            dayn = num(&mut i, 2)?;
        }
    }
    let (mut h, mut m, mut sec, mut ms) = (0.0, 0.0, 0.0, 0.0);
    let mut date_only = true;
    if b.get(i) == Some(&b'T') || b.get(i) == Some(&b't') || (b.get(i) == Some(&b' ') && i + 3 < b.len() && b[i + 3] == b':') {
        date_only = false;
        i += 1;
        h = num(&mut i, 2)?;
        if b.get(i) != Some(&b':') {
            return None;
        }
        i += 1;
        m = num(&mut i, 2)?;
        if b.get(i) == Some(&b':') {
            i += 1;
            sec = num(&mut i, 2)?;
            if b.get(i) == Some(&b'.') || b.get(i) == Some(&b',') {
                i += 1;
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                let frac = &s[start..i];
                if frac.is_empty() {
                    return None;
                }
                ms = (format!("0.{frac}").parse::<f64>().ok()? * 1000.0).floor();
            }
        }
    }
    let mut offset = 0.0;
    if i < b.len() {
        match b[i] {
            b'Z' | b'z' => i += 1,
            b'+' | b'-' => {
                let sign = if b[i] == b'-' { -1.0 } else { 1.0 };
                i += 1;
                let oh = num(&mut i, 2)?;
                if b.get(i) == Some(&b':') {
                    i += 1;
                }
                let om = num(&mut i, 2).unwrap_or(0.0);
                offset = sign * (oh * 60.0 + om) * 60_000.0;
            }
            _ => return None,
        }
    }
    let _ = date_only;
    if i != b.len() || !(1.0..=12.0).contains(&month) || !(1.0..=31.0).contains(&dayn) || h > 24.0 || m > 59.0 || sec > 59.0 {
        return None;
    }
    let t = make_date(make_day(year, month - 1.0, dayn), make_time(h, m, sec, ms)) - offset;
    Some(time_clip(t))
}

/// "Tue Jan 01 2024 10:00:00 GMT+0100", "January 1, 2024", "1 Jan 2024
/// 10:00 UTC", "2024/01/01 10:00"
fn parse_loose(s: &str) -> Option<f64> {
    let mut year = None;
    let mut month = None;
    let mut dayn = None;
    let (mut h, mut m, mut sec) = (0.0, 0.0, 0.0);
    let mut offset = 0.0;
    let mut pm = None;
    let cleaned: String = s.chars().map(|c| if c == ',' { ' ' } else { c }).collect();
    let mut tokens = cleaned.split_whitespace().peekable();
    while let Some(tok) = tokens.next() {
        let lower = tok.to_ascii_lowercase();
        if let Some(mi) = MONTHS.iter().position(|mn| lower.starts_with(&mn.to_ascii_lowercase())) {
            month = Some(mi as f64);
            continue;
        }
        if DAYS.iter().any(|d| lower.starts_with(&d.to_ascii_lowercase())) {
            continue;
        }
        if lower == "am" || lower == "pm" {
            pm = Some(lower == "pm");
            continue;
        }
        if lower.starts_with("gmt") || lower.starts_with("utc") || lower == "z" {
            let rest = &tok[3.min(tok.len())..];
            if let Some(o) = parse_offset(rest) {
                offset = o;
            }
            continue;
        }
        if tok.starts_with('(') {
            // "(Coordinated Universal Time)": skip to the closing paren
            let mut t = tok;
            while !t.ends_with(')') {
                match tokens.next() {
                    Some(n) => t = n,
                    None => break,
                }
            }
            continue;
        }
        if (tok.starts_with('+') || tok.starts_with('-')) && tok.len() >= 5 {
            if let Some(o) = parse_offset(tok) {
                offset = o;
                continue;
            }
        }
        if tok.contains(':') {
            let parts: Vec<&str> = tok.split(':').collect();
            h = parts.first()?.parse().ok()?;
            m = parts.get(1).map(|p| p.parse().unwrap_or(0.0)).unwrap_or(0.0);
            sec = parts.get(2).map(|p| p.parse().unwrap_or(0.0)).unwrap_or(0.0);
            continue;
        }
        if tok.contains('/') || (tok.contains('-') && tok.len() > 3) {
            let sep = if tok.contains('/') { '/' } else { '-' };
            let parts: Vec<f64> = tok.split(sep).filter_map(|p| p.parse().ok()).collect();
            if parts.len() == 3 {
                if parts[0] > 31.0 {
                    year = Some(parts[0]);
                    month = Some(parts[1] - 1.0);
                    dayn = Some(parts[2]);
                } else {
                    month = Some(parts[0] - 1.0);
                    dayn = Some(parts[1]);
                    year = Some(parts[2]);
                }
                continue;
            }
            return None;
        }
        if let Ok(n) = tok.parse::<f64>() {
            if dayn.is_none() && n <= 31.0 && (year.is_some() || month.is_some() || tokens.peek().is_some()) {
                dayn = Some(n);
            } else if year.is_none() {
                year = Some(if n < 50.0 { n + 2000.0 } else if n < 100.0 { n + 1900.0 } else { n });
            } else {
                return None;
            }
            continue;
        }
        return None;
    }
    if let Some(p) = pm {
        if p && h < 12.0 {
            h += 12.0;
        } else if !p && h == 12.0 {
            h = 0.0;
        }
    }
    let t = make_date(make_day(year?, month?, dayn.unwrap_or(1.0)), make_time(h, m, sec, 0.0)) - offset;
    Some(time_clip(t))
}

fn parse_offset(s: &str) -> Option<f64> {
    if s.is_empty() {
        return Some(0.0);
    }
    let sign = match s.as_bytes()[0] {
        b'+' => 1.0,
        b'-' => -1.0,
        _ => return None,
    };
    let digits: String = s[1..].chars().filter(|c| c.is_ascii_digit()).collect();
    let (h, m) = match digits.len() {
        1 | 2 => (digits.parse::<f64>().ok()?, 0.0),
        4 => (digits[..2].parse::<f64>().ok()?, digits[2..].parse::<f64>().ok()?),
        _ => return None,
    };
    Some(sign * (h * 60.0 + m) * 60_000.0)
}

// ---- formatting ----

fn two(n: f64) -> String {
    format!("{:02}", n as i64)
}

fn year_str(y: f64) -> String {
    if y < 0.0 { format!("-{:06}", -y as i64) } else { format!("{:04}", y as i64) }
}

fn date_part(t: f64) -> String {
    format!("{} {} {} {}", DAYS[week_day(t) as usize], MONTHS[month_from_time(t) as usize], two(date_from_time(t)), year_str(year_from_time(t)))
}

fn time_part(t: f64) -> String {
    format!("{}:{}:{} GMT+0000 (Coordinated Universal Time)", two(hour_from_time(t)), two(min_from_time(t)), two(sec_from_time(t)))
}

pub(crate) fn iso_string(t: f64) -> String {
    let y = year_from_time(t);
    let ys = if (0.0..=9999.0).contains(&y) { format!("{:04}", y as i64) } else if y < 0.0 { format!("-{:06}", -y as i64) } else { format!("+{:06}", y as i64) };
    format!(
        "{}-{}-{}T{}:{}:{}.{:03}Z",
        ys,
        two(month_from_time(t) + 1.0),
        two(date_from_time(t)),
        two(hour_from_time(t)),
        two(min_from_time(t)),
        two(sec_from_time(t)),
        ms_from_time(t) as i64
    )
}

// ---- built-ins ----

fn this_time(vm: &mut Vm, this: Value) -> JsResult<f64> {
    if let Some(o) = this.as_object() {
        if let ObjectKind::Date(t) = o.get().kind {
            return Ok(t);
        }
    }
    Err(vm.type_error("this is not a Date object."))
}

fn set_this_time(this: Value, t: f64) {
    if let Some(o) = this.as_object() {
        if let ObjectKind::Date(v) = &mut o.get_mut().kind {
            *v = t;
        }
    }
}

fn date_call(vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let t = now_ms();
    Ok(vm.str_value(&format!("{} {}", date_part(t), time_part(t))))
}

fn components(vm: &mut Vm, args: &[Value]) -> JsResult<f64> {
    let mut nums = [f64::NAN, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    for (i, &a) in args.iter().take(7).enumerate() {
        nums[i] = vm.to_number(a)?;
    }
    let mut y = nums[0];
    if !y.is_nan() {
        let yi = y.trunc();
        if (0.0..=99.0).contains(&yi) {
            y = 1900.0 + yi;
        }
    }
    Ok(make_date(make_day(y, nums[1], nums[2]), make_time(nums[3], nums[4], nums[5], nums[6])))
}

fn date_construct(vm: &mut Vm, new_target: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let t = match args.len() {
        0 => now_ms(),
        1 => {
            let v = args[0];
            let date = v.as_object().and_then(|o| match o.get().kind {
                ObjectKind::Date(t) => Some(t),
                _ => None,
            });
            let v = match date {
                Some(t) => Value::number(t),
                None => vm.to_primitive(v, Hint::Default)?,
            };
            if let Some(s) = v.as_string() {
                parse_date(&s.get().to_rust_string())
            } else {
                time_clip(vm.to_number(v)?)
            }
        }
        _ => time_clip(components(vm, args)?),
    };
    let proto = vm.prototype_for(new_target, |r| r.date_proto)?;
    Ok(Value::object(vm.new_object_with(Some(proto), ObjectKind::Date(t))))
}

fn now(_vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(now_ms()))
}

fn parse(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.to_rust_string(arg(args, 0))?;
    Ok(Value::number(parse_date(&s)))
}

fn utc(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(time_clip(components(vm, args)?)))
}

fn get_time(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(this_time(vm, this)?))
}

fn field(vm: &mut Vm, this: Value, f: fn(f64) -> f64) -> JsResult<Value> {
    let t = this_time(vm, this)?;
    if t.is_nan() {
        return Ok(Value::NAN);
    }
    Ok(Value::number(f(t)))
}

fn set_time(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    this_time(vm, this)?;
    let t = time_clip(vm.to_number(arg(args, 0))?);
    set_this_time(this, t);
    Ok(Value::number(t))
}

/// Setters: `first` is the first field set (0 year .. 6 ms); following
/// arguments set the following fields (as far as each method allows)
fn set_fields(vm: &mut Vm, this: Value, args: &[Value], first: usize) -> JsResult<Value> {
    let t = this_time(vm, this)?;
    let limit = match first {
        0 => 3,
        1 => 2,
        2 => 1,
        3 => 4,
        4 => 3,
        5 => 2,
        _ => 1,
    };
    let base = if t.is_nan() && first == 0 { 0.0 } else { t };
    let mut f = [
        year_from_time(base),
        month_from_time(base),
        date_from_time(base),
        hour_from_time(base),
        min_from_time(base),
        sec_from_time(base),
        ms_from_time(base),
    ];
    for k in 0..limit.min(args.len().max(1)) {
        f[first + k] = vm.to_number(arg(args, k))?;
    }
    if t.is_nan() && first != 0 {
        return Ok(Value::NAN);
    }
    let nt = time_clip(make_date(make_day(f[0], f[1], f[2]), make_time(f[3], f[4], f[5], f[6])));
    set_this_time(this, nt);
    Ok(Value::number(nt))
}

fn to_iso_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let t = this_time(vm, this)?;
    if t.is_nan() {
        return Err(vm.range_error("Invalid time value"));
    }
    Ok(vm.str_value(&iso_string(t)))
}

fn to_json(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let p = vm.to_primitive(this, Hint::Number)?;
    if p.as_number().is_some_and(|n| !n.is_finite()) {
        return Ok(Value::NULL);
    }
    let f = vm.get_str(this, "toISOString")?;
    vm.call(f, this, &[])
}

fn formatted(vm: &mut Vm, this: Value, f: fn(f64) -> String) -> JsResult<Value> {
    let t = this_time(vm, this)?;
    if t.is_nan() {
        return Ok(vm.str_value("Invalid Date"));
    }
    Ok(vm.str_value(&f(t)))
}

fn to_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    formatted(vm, this, |t| format!("{} {}", date_part(t), time_part(t)))
}

fn to_date_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    formatted(vm, this, date_part)
}

fn to_time_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    formatted(vm, this, time_part)
}

fn to_utc_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    formatted(vm, this, |t| {
        format!(
            "{}, {} {} {} {}:{}:{} GMT",
            DAYS[week_day(t) as usize],
            two(date_from_time(t)),
            MONTHS[month_from_time(t) as usize],
            year_str(year_from_time(t)),
            two(hour_from_time(t)),
            two(min_from_time(t)),
            two(sec_from_time(t))
        )
    })
}

fn locale_date(t: f64) -> String {
    format!("{}/{}/{}", month_from_time(t) as i64 + 1, date_from_time(t) as i64, year_from_time(t) as i64)
}

fn locale_time(t: f64) -> String {
    let h = hour_from_time(t) as i64;
    let (h12, ampm) = if h == 0 { (12, "AM") } else if h < 12 { (h, "AM") } else if h == 12 { (12, "PM") } else { (h - 12, "PM") };
    format!("{}:{}:{} {}", h12, two(min_from_time(t)), two(sec_from_time(t)), ampm)
}

fn to_locale_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    formatted(vm, this, |t| format!("{}, {}", locale_date(t), locale_time(t)))
}

fn to_locale_date_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    formatted(vm, this, locale_date)
}

fn to_locale_time_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    formatted(vm, this, locale_time)
}

fn to_primitive(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if !this.is_object() {
        return Err(vm.type_error("Date.prototype[Symbol.toPrimitive] called on non-object"));
    }
    let hint = vm.to_rust_string(arg(args, 0))?;
    let order = match hint.as_str() {
        "string" | "default" => ["toString", "valueOf"],
        "number" => ["valueOf", "toString"],
        _ => return Err(vm.type_error("Invalid hint")),
    };
    for name in order {
        let f = vm.get_str(this, name)?;
        if vm.is_callable(f) {
            let r = vm.call(f, this, &[])?;
            if !r.is_object() {
                return Ok(r);
            }
        }
    }
    Err(vm.type_error("Cannot convert object to primitive value"))
}
