// Intl, English only
//
// Every locale resolves to "en-US": the formats are the ones en-US uses,
// which is what pages fall back to and what most of them ask for. The
// object is built the first time a script touches `Intl` (see intl.rs).
(() => {
  'use strict';
  const LOCALE = 'en-US';

  const canonicalize = (locales) => {
    if (locales === undefined) return [];
    const list = typeof locales === 'string' || (locales && typeof locales === 'object' && 'baseName' in locales) ? [locales] : Array.from(locales);
    return list.map((l) => {
      const s = typeof l === 'object' && l ? String(l.baseName) : l;
      if (typeof s !== 'string') throw new TypeError('Language ID should be string or object.');
      if (!/^[A-Za-z]{2,8}(-[A-Za-z0-9]{1,8})*$/.test(s)) throw new RangeError(`Incorrect locale information provided: ${s}`);
      const parts = s.split('-');
      return parts.map((p, i) => (i === 0 ? p.toLowerCase() : p.length === 2 ? p.toUpperCase() : p.length === 4 ? p[0].toUpperCase() + p.slice(1).toLowerCase() : p.toLowerCase())).join('-');
    });
  };
  const option = (options, name, allowed, fallback) => {
    const v = options[name];
    if (v === undefined) return fallback;
    const s = typeof fallback === 'boolean' ? Boolean(v) : String(v);
    if (allowed && !allowed.includes(s)) throw new RangeError(`Value ${s} out of range for Intl options property ${name}`);
    return s;
  };
  const numberOption = (options, name, min, max, fallback) => {
    const v = options[name];
    if (v === undefined) return fallback;
    const n = Number(v);
    if (!Number.isFinite(n) || n < min || n > max) throw new RangeError(`${name} value is out of range.`);
    return Math.floor(n);
  };
  const toOptions = (o) => (o === undefined ? {} : Object(o));

  // ---- NumberFormat ----

  const CURRENCY = { USD: '$', EUR: '€', GBP: '£', JPY: '¥', CNY: 'CN¥', INR: '₹', KRW: '₩', CAD: 'CA$', AUD: 'A$', MXN: 'MX$', BRL: 'R$', CHF: 'CHF', RUB: 'RUB', ILS: '₪', VND: '₫', TWD: 'NT$', NZD: 'NZ$', HKD: 'HK$' };
  const ZERO_DECIMAL = new Set(['JPY', 'KRW', 'VND', 'CLP', 'ISK', 'HUF', 'TWD']);
  const UNITS = { percent: '%', byte: 'byte', kilobyte: 'kB', megabyte: 'MB', gigabyte: 'GB', terabyte: 'TB', second: 'sec', minute: 'min', hour: 'hr', day: 'day', week: 'wk', month: 'mth', year: 'yr', millisecond: 'ms', meter: 'm', kilometer: 'km', centimeter: 'cm', kilogram: 'kg', gram: 'g', celsius: '°C', fahrenheit: '°F', liter: 'L', mile: 'mi', foot: 'ft', inch: 'in' };

  // Round |x| to `max` fraction digits (half away from zero, on the
  // decimal digits), keep at least `min`
  function fixedDigits(x, min, max) {
    let s = x.toFixed(Math.min(max, 100));
    if (Number(s) === 0 && x !== 0 && max < 20) s = (0).toFixed(max);
    if (s.includes('.')) {
      let [int, frac] = s.split('.');
      while (frac.length > min && frac.endsWith('0')) frac = frac.slice(0, -1);
      return frac ? `${int}.${frac}` : int;
    }
    return min > 0 ? `${s}.${'0'.repeat(min)}` : s;
  }
  function significantDigits(x, min, max) {
    if (x === 0) return min > 1 ? `0.${'0'.repeat(min - 1)}` : '0';
    let s = Number(x.toPrecision(max)).toString();
    if (s.includes('e')) s = Number(s).toFixed(Math.max(0, max - Math.floor(Math.log10(x)) - 1));
    const digits = s.replace('.', '').replace(/^0+/, '').length;
    if (digits < min) s = Number(s).toPrecision(min);
    return s;
  }
  const group = (int, useGrouping) => (useGrouping && int.length > 3 ? int.replace(/\B(?=(\d{3})+(?!\d))/g, ',') : int);

  class NumberFormat {
    #o;
    constructor(locales, options) {
      canonicalize(locales);
      options = toOptions(options);
      const style = option(options, 'style', ['decimal', 'percent', 'currency', 'unit'], 'decimal');
      const currency = options.currency === undefined ? undefined : String(options.currency).toUpperCase();
      if (style === 'currency' && !currency) throw new TypeError('Currency code is required with currency style.');
      const unit = options.unit === undefined ? undefined : String(options.unit);
      if (style === 'unit' && !unit) throw new TypeError('Unit is required with unit style.');
      const notation = option(options, 'notation', ['standard', 'scientific', 'engineering', 'compact'], 'standard');
      const currencyDigits = currency && ZERO_DECIMAL.has(currency) ? 0 : 2;
      const defMin = style === 'currency' && notation !== 'compact' ? currencyDigits : 0;
      const defMax = style === 'currency' && notation !== 'compact' ? currencyDigits : style === 'percent' || notation === 'compact' ? 0 : 3;
      const minFD = numberOption(options, 'minimumFractionDigits', 0, 100, undefined);
      const maxFD = numberOption(options, 'maximumFractionDigits', 0, 100, undefined);
      const minSD = numberOption(options, 'minimumSignificantDigits', 1, 21, undefined);
      const maxSD = numberOption(options, 'maximumSignificantDigits', 1, 21, undefined);
      const minimumFractionDigits = minFD ?? Math.min(defMin, maxFD ?? defMin);
      const maximumFractionDigits = Math.max(maxFD ?? Math.max(defMax, minimumFractionDigits), minimumFractionDigits);
      if (minFD !== undefined && maxFD !== undefined && minFD > maxFD) throw new RangeError('maximumFractionDigits value is out of range.');
      const ug = options.useGrouping;
      this.#o = {
        locale: LOCALE, numberingSystem: 'latn', style, currency,
        currencyDisplay: option(options, 'currencyDisplay', ['symbol', 'narrowSymbol', 'code', 'name'], 'symbol'),
        unit, unitDisplay: option(options, 'unitDisplay', ['short', 'narrow', 'long'], 'short'),
        minimumIntegerDigits: numberOption(options, 'minimumIntegerDigits', 1, 21, 1),
        minimumFractionDigits, maximumFractionDigits,
        minimumSignificantDigits: minSD ?? (maxSD !== undefined ? 1 : undefined),
        maximumSignificantDigits: maxSD ?? (minSD !== undefined ? 21 : undefined),
        useGrouping: ug === undefined ? (notation === 'compact' ? 'min2' : 'auto') : ug === false || ug === 'false' ? false : ug === 'min2' ? 'min2' : 'auto',
        notation, compactDisplay: option(options, 'compactDisplay', ['short', 'long'], 'short'),
        signDisplay: option(options, 'signDisplay', ['auto', 'never', 'always', 'exceptZero', 'negative'], 'auto'),
      };
    }
    resolvedOptions() {
      const r = {};
      for (const [k, v] of Object.entries(this.#o)) if (v !== undefined) r[k] = v;
      return r;
    }
    format(x) { return this.formatToParts(x).map((p) => p.value).join(''); }
    formatRange(a, b) { return `${this.format(a)}–${this.format(b)}`; }
    formatToParts(x) {
      const o = this.#o;
      const big = typeof x === 'bigint';
      let n = big ? Number(x) : Number(x);
      const parts = [];
      const negative = n < 0 || Object.is(n, -0);
      if (Number.isNaN(n)) return [{ type: 'nan', value: 'NaN' }];
      let abs = Math.abs(n);
      if (o.style === 'percent') abs *= 100;
      let suffix = '';
      let exponent = null;
      if (Number.isFinite(abs) && abs !== 0) {
        if (o.notation === 'compact') {
          const scales = o.compactDisplay === 'long' ? [[1e12, ' trillion'], [1e9, ' billion'], [1e6, ' million'], [1e3, ' thousand']] : [[1e12, 'T'], [1e9, 'B'], [1e6, 'M'], [1e3, 'K']];
          for (const [scale, label] of scales) {
            if (abs >= scale * 0.9995 && Number((abs / scale).toPrecision(abs / scale < 100 ? 2 : 3)) >= 1) {
              abs /= scale;
              suffix = label;
              break;
            }
          }
        } else if (o.notation === 'scientific' || o.notation === 'engineering') {
          let e = Math.floor(Math.log10(abs));
          if (o.notation === 'engineering') e -= ((e % 3) + 3) % 3;
          abs /= 10 ** e;
          exponent = e;
        }
      }
      let digits;
      if (!Number.isFinite(abs)) digits = '∞';
      else if (o.maximumSignificantDigits !== undefined) digits = significantDigits(abs, o.minimumSignificantDigits, o.maximumSignificantDigits);
      else if (o.notation === 'compact' && o.maximumFractionDigits === 0 && abs < 100) digits = significantDigits(abs, 1, 2);
      else digits = fixedDigits(abs, o.minimumFractionDigits, o.maximumFractionDigits);
      if (big && o.maximumSignificantDigits === undefined && o.notation === 'standard' && o.style !== 'percent') {
        digits = (x < 0n ? -x : x).toString() + (o.minimumFractionDigits ? `.${'0'.repeat(o.minimumFractionDigits)}` : '');
      }
      const isZero = !/[1-9]/.test(digits);
      let [int, frac] = digits.split('.');
      if (int !== '∞') int = int.padStart(o.minimumIntegerDigits, '0');
      const sign = (() => {
        switch (o.signDisplay) {
          case 'never': return '';
          case 'always': return negative ? '-' : '+';
          case 'exceptZero': return isZero ? '' : negative ? '-' : '+';
          case 'negative': return negative && !isZero ? '-' : '';
          default: return negative ? '-' : '';
        }
      })();
      if (sign) parts.push({ type: sign === '-' ? 'minusSign' : 'plusSign', value: sign });
      if (o.style === 'currency' && o.currencyDisplay !== 'name') {
        const sym = o.currencyDisplay === 'code' ? `${o.currency} ` : o.currencyDisplay === 'narrowSymbol' ? (CURRENCY[o.currency] || o.currency).replace(/^[A-Z]+(?=\W)/, '') : CURRENCY[o.currency] || `${o.currency} `;
        parts.push({ type: 'currency', value: sym });
      }
      if (int === '∞') parts.push({ type: 'infinity', value: '∞' });
      else {
        const grouped = o.useGrouping === false || (o.useGrouping === 'min2' && int.length < 5) ? int : group(int, true);
        grouped.split(',').forEach((g, i) => {
          if (i) parts.push({ type: 'group', value: ',' });
          parts.push({ type: 'integer', value: g });
        });
      }
      if (frac) parts.push({ type: 'decimal', value: '.' }, { type: 'fraction', value: frac });
      if (exponent !== null) parts.push({ type: 'exponentSeparator', value: 'E' }, { type: 'exponentInteger', value: String(exponent) });
      if (suffix) {
        if (suffix.startsWith(' ')) parts.push({ type: 'literal', value: ' ' });
        parts.push({ type: 'compact', value: suffix.trim() });
      }
      if (o.style === 'percent') parts.push({ type: 'percentSign', value: '%' });
      if (o.style === 'currency' && o.currencyDisplay === 'name') parts.push({ type: 'literal', value: ' ' }, { type: 'currency', value: `${o.currency}` });
      if (o.style === 'unit') {
        const short = UNITS[o.unit] || o.unit;
        const label = o.unitDisplay === 'long' ? ` ${o.unit}${digits === '1' ? '' : 's'}` : o.unit === 'percent' ? short : o.unitDisplay === 'narrow' ? short : ` ${short}`;
        if (label.startsWith(' ')) parts.push({ type: 'literal', value: ' ' }, { type: 'unit', value: label.slice(1) });
        else parts.push({ type: 'unit', value: label });
      }
      return parts;
    }
    static supportedLocalesOf(locales) { return canonicalize(locales).filter((l) => l.startsWith('en')); }
  }
  // `format` is a bound getter in the spec
  Object.defineProperty(NumberFormat.prototype, 'format', {
    configurable: true,
    get() {
      const f = NumberFormat.prototype.formatToParts;
      const bound = (x) => f.call(this, x).map((p) => p.value).join('');
      Object.defineProperty(this, 'format', { value: bound, configurable: true });
      return bound;
    },
  });

  // ---- DateTimeFormat ----

  const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
  const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
  const pad2 = (n) => String(n).padStart(2, '0');

  class DateTimeFormat {
    #o;
    constructor(locales, options) {
      canonicalize(locales);
      options = toOptions(options);
      const o = { locale: LOCALE, calendar: 'gregory', numberingSystem: 'latn', timeZone: 'UTC' };
      if (options.timeZone !== undefined) {
        const tz = String(options.timeZone);
        if (!/^(UTC|GMT|Etc\/UTC|Etc\/GMT)$/i.test(tz) && !/^[A-Za-z_]+\/[A-Za-z_\/+-]+$/.test(tz)) throw new RangeError(`Invalid time zone specified: ${tz}`);
        // Only UTC is modeled; other zones are reported but shown in UTC
        o.timeZone = /^(utc|gmt|etc\/utc|etc\/gmt)$/i.test(tz) ? 'UTC' : tz;
      }
      const fields = {
        weekday: ['narrow', 'short', 'long'], era: ['narrow', 'short', 'long'], year: ['numeric', '2-digit'],
        month: ['numeric', '2-digit', 'narrow', 'short', 'long'], day: ['numeric', '2-digit'], hour: ['numeric', '2-digit'],
        minute: ['numeric', '2-digit'], second: ['numeric', '2-digit'], timeZoneName: ['short', 'long', 'shortOffset', 'longOffset', 'shortGeneric', 'longGeneric'],
      };
      let any = false;
      for (const [k, allowed] of Object.entries(fields)) {
        const v = option(options, k, allowed, undefined);
        if (v !== undefined) { o[k] = v; any = true; }
      }
      const fsd = numberOption(options, 'fractionalSecondDigits', 1, 3, undefined);
      if (fsd !== undefined) { o.fractionalSecondDigits = fsd; any = true; }
      const dateStyle = option(options, 'dateStyle', ['full', 'long', 'medium', 'short'], undefined);
      const timeStyle = option(options, 'timeStyle', ['full', 'long', 'medium', 'short'], undefined);
      if ((dateStyle || timeStyle) && any) throw new TypeError("Can't set option dateStyle/timeStyle when other date-time fields are set");
      if (dateStyle) o.dateStyle = dateStyle;
      if (timeStyle) o.timeStyle = timeStyle;
      if (!any && !dateStyle && !timeStyle) Object.assign(o, { year: 'numeric', month: 'numeric', day: 'numeric' });
      const h12 = options.hour12;
      const hourCycle = option(options, 'hourCycle', ['h11', 'h12', 'h23', 'h24'], undefined);
      if (o.hour || timeStyle) o.hourCycle = h12 !== undefined ? (h12 ? 'h12' : 'h23') : hourCycle || 'h12';
      if (o.hourCycle) o.hour12 = o.hourCycle === 'h12' || o.hourCycle === 'h11';
      this.#o = o;
    }
    resolvedOptions() { return { ...this.#o }; }
    format(d) { return this.formatToParts(d).map((p) => p.value).join(''); }
    formatRange(a, b) { return `${this.format(a)} – ${this.format(b)}`; }
    formatToParts(d) {
      const t = d === undefined ? Date.now() : Number(d instanceof Date ? d.getTime() : d);
      if (!Number.isFinite(t)) throw new RangeError('Invalid time value');
      const date = new Date(t);
      const y = date.getUTCFullYear(), mo = date.getUTCMonth(), day = date.getUTCDate(), wd = date.getUTCDay();
      const h = date.getUTCHours(), mi = date.getUTCMinutes(), s = date.getUTCSeconds(), ms = date.getUTCMilliseconds();
      let o = this.#o;
      if (o.dateStyle || o.timeStyle) {
        const ds = { full: { weekday: 'long', month: 'long', day: 'numeric', year: 'numeric' }, long: { month: 'long', day: 'numeric', year: 'numeric' }, medium: { month: 'short', day: 'numeric', year: 'numeric' }, short: { month: 'numeric', day: 'numeric', year: '2-digit' } }[o.dateStyle] || {};
        const ts = { full: { hour: 'numeric', minute: '2-digit', second: '2-digit', timeZoneName: 'long' }, long: { hour: 'numeric', minute: '2-digit', second: '2-digit', timeZoneName: 'short' }, medium: { hour: 'numeric', minute: '2-digit', second: '2-digit' }, short: { hour: 'numeric', minute: '2-digit' } }[o.timeStyle] || {};
        o = { ...o, ...ds, ...ts, styled: true };
      }
      const parts = [];
      const lit = (v) => parts.push({ type: 'literal', value: v });
      const part = (type, value) => parts.push({ type, value: String(value) });
      const textMonth = o.month === 'long' || o.month === 'short' || o.month === 'narrow';
      const monthText = o.month === 'long' ? MONTHS[mo] : o.month === 'short' ? MONTHS[mo].slice(0, 3) : MONTHS[mo][0];
      const yearText = o.year === '2-digit' ? pad2(y % 100) : String(y);
      const hasDate = o.year || o.month || o.day || o.weekday;
      if (o.weekday) {
        part('weekday', o.weekday === 'long' ? DAYS[wd] : o.weekday === 'short' ? DAYS[wd].slice(0, 3) : DAYS[wd][0]);
        if (o.month || o.day || o.year) lit(', ');
      }
      if (textMonth) {
        if (o.month) part('month', monthText);
        if (o.day) { lit(' '); part('day', o.day === '2-digit' ? pad2(day) : day); }
        if (o.year) { lit(o.day ? ', ' : ' '); part('year', yearText); }
      } else {
        const seq = [];
        if (o.month) seq.push(['month', o.month === '2-digit' ? pad2(mo + 1) : mo + 1]);
        if (o.day) seq.push(['day', o.day === '2-digit' ? pad2(day) : day]);
        if (o.year) seq.push(['year', yearText]);
        seq.forEach(([k, v], i) => { if (i) lit('/'); part(k, v); });
      }
      if (o.era) { lit(' '); part('era', y > 0 ? (o.era === 'long' ? 'Anno Domini' : 'AD') : (o.era === 'long' ? 'Before Christ' : 'BC')); }
      if (o.hour || o.minute || o.second) {
        if (hasDate) lit(o.styled && (o.dateStyle === 'full' || o.dateStyle === 'long') ? ' at ' : ', ');
        const twelve = o.hourCycle === 'h12' || o.hourCycle === 'h11';
        if (o.hour) {
          let hh = twelve ? h % 12 || 12 : h;
          if (o.hourCycle === 'h11') hh = h % 12;
          part('hour', o.hour === '2-digit' || (!twelve && (o.minute || o.styled)) ? pad2(hh) : hh);
        }
        if (o.minute) { if (o.hour) lit(':'); part('minute', o.minute === '2-digit' || o.hour ? pad2(mi) : mi); }
        if (o.second) { if (o.hour || o.minute) lit(':'); part('second', o.second === '2-digit' || o.minute ? pad2(s) : s); }
        if (o.fractionalSecondDigits) { lit('.'); part('fractionalSecond', String(ms).padStart(3, '0').slice(0, o.fractionalSecondDigits)); }
        if (o.hour && twelve) { lit(' '); part('dayPeriod', h < 12 ? 'AM' : 'PM'); }
      }
      if (o.timeZoneName) { lit(' '); part('timeZoneName', o.timeZoneName === 'long' ? 'Coordinated Universal Time' : 'UTC'); }
      return parts;
    }
    static supportedLocalesOf(locales) { return canonicalize(locales).filter((l) => l.startsWith('en')); }
  }
  Object.defineProperty(DateTimeFormat.prototype, 'format', {
    configurable: true,
    get() {
      const f = DateTimeFormat.prototype.formatToParts;
      const bound = (d) => f.call(this, d).map((p) => p.value).join('');
      Object.defineProperty(this, 'format', { value: bound, configurable: true });
      return bound;
    },
  });

  // ---- PluralRules ----

  class PluralRules {
    #type;
    constructor(locales, options) {
      canonicalize(locales);
      this.#type = option(toOptions(options), 'type', ['cardinal', 'ordinal'], 'cardinal');
    }
    select(n) {
      n = Number(n);
      if (this.#type === 'ordinal') {
        const t = Math.abs(n) % 100, u = Math.abs(n) % 10;
        if (u === 1 && t !== 11) return 'one';
        if (u === 2 && t !== 12) return 'two';
        if (u === 3 && t !== 13) return 'few';
        return 'other';
      }
      return n === 1 ? 'one' : 'other';
    }
    resolvedOptions() { return { locale: LOCALE, type: this.#type, minimumIntegerDigits: 1, minimumFractionDigits: 0, maximumFractionDigits: 3, pluralCategories: this.#type === 'ordinal' ? ['few', 'one', 'two', 'other'] : ['one', 'other'] }; }
    static supportedLocalesOf(locales) { return canonicalize(locales).filter((l) => l.startsWith('en')); }
  }

  // ---- RelativeTimeFormat ----

  const RT_UNITS = ['year', 'quarter', 'month', 'week', 'day', 'hour', 'minute', 'second'];
  const RT_SHORT = { year: 'yr.', quarter: 'qtr.', month: 'mo.', week: 'wk.', day: 'day', hour: 'hr.', minute: 'min.', second: 'sec.' };
  class RelativeTimeFormat {
    #o;
    constructor(locales, options) {
      canonicalize(locales);
      options = toOptions(options);
      this.#o = { locale: LOCALE, style: option(options, 'style', ['long', 'short', 'narrow'], 'long'), numeric: option(options, 'numeric', ['always', 'auto'], 'always'), numberingSystem: 'latn' };
    }
    format(value, unit) { return this.formatToParts(value, unit).map((p) => p.value).join(''); }
    formatToParts(value, unit) {
      value = Number(value);
      if (!Number.isFinite(value)) throw new RangeError('Invalid value');
      unit = String(unit).replace(/s$/, '');
      if (!RT_UNITS.includes(unit)) throw new RangeError(`Invalid unit argument for format() '${unit}'`);
      const o = this.#o;
      if (o.numeric === 'auto') {
        const words = { day: { '-1': 'yesterday', 0: 'today', 1: 'tomorrow' }, year: { '-1': 'last year', 0: 'this year', 1: 'next year' }, month: { '-1': 'last month', 0: 'this month', 1: 'next month' }, week: { '-1': 'last week', 0: 'this week', 1: 'next week' }, quarter: { '-1': 'last quarter', 0: 'this quarter', 1: 'next quarter' }, hour: { 0: 'this hour' }, minute: { 0: 'this minute' }, second: { 0: 'now' } };
        const w = words[unit]?.[Object.is(value, -0) ? 0 : value];
        if (w) return [{ type: 'literal', value: w }];
      }
      const abs = Math.abs(value);
      const num = new NumberFormat().format(abs);
      const label = o.style === 'long' ? unit + (abs === 1 ? '' : 's') : RT_SHORT[unit];
      const neg = value < 0 || Object.is(value, -0);
      const parts = [{ type: 'integer', value: num, unit }];
      return neg ? [...parts, { type: 'literal', value: ` ${label} ago` }] : [{ type: 'literal', value: 'in ' }, ...parts, { type: 'literal', value: ` ${label}` }];
    }
    resolvedOptions() { return { ...this.#o }; }
    static supportedLocalesOf(locales) { return canonicalize(locales).filter((l) => l.startsWith('en')); }
  }

  // ---- ListFormat ----

  class ListFormat {
    #o;
    constructor(locales, options) {
      canonicalize(locales);
      options = toOptions(options);
      this.#o = { locale: LOCALE, type: option(options, 'type', ['conjunction', 'disjunction', 'unit'], 'conjunction'), style: option(options, 'style', ['long', 'short', 'narrow'], 'long') };
    }
    format(list) { return this.formatToParts(list).map((p) => p.value).join(''); }
    formatToParts(list) {
      const items = Array.from(list, (x) => { if (typeof x !== 'string') throw new TypeError('Iterable yielded a non-string'); return x; });
      const { type, style } = this.#o;
      const word = type === 'disjunction' ? 'or' : style === 'long' ? 'and' : style === 'short' ? '&' : '';
      const parts = [];
      items.forEach((it, i) => {
        if (i > 0) {
          const last = i === items.length - 1;
          const sep = type === 'unit' ? (style === 'narrow' ? ' ' : ', ') : !last ? ', ' : items.length === 2 ? (word ? ` ${word} ` : ', ') : word ? `, ${word} ` : ', ';
          parts.push({ type: 'literal', value: sep });
        }
        parts.push({ type: 'element', value: it });
      });
      return parts;
    }
    resolvedOptions() { return { ...this.#o }; }
    static supportedLocalesOf(locales) { return canonicalize(locales).filter((l) => l.startsWith('en')); }
  }

  // ---- Collator ----

  class Collator {
    #o;
    constructor(locales, options) {
      canonicalize(locales);
      options = toOptions(options);
      this.#o = { locale: LOCALE, usage: option(options, 'usage', ['sort', 'search'], 'sort'), sensitivity: option(options, 'sensitivity', ['base', 'accent', 'case', 'variant'], 'variant'), ignorePunctuation: option(options, 'ignorePunctuation', null, false), collation: 'default', numeric: option(options, 'numeric', null, false), caseFirst: option(options, 'caseFirst', ['upper', 'lower', 'false'], 'false') };
    }
    get compare() {
      const o = this.#o;
      const cmp = (a, b) => String(a).localeCompare(String(b), undefined, o);
      Object.defineProperty(this, 'compare', { value: cmp, configurable: true });
      return cmp;
    }
    resolvedOptions() { return { ...this.#o }; }
    static supportedLocalesOf(locales) { return canonicalize(locales).filter((l) => l.startsWith('en')); }
  }

  // ---- Segmenter ----

  class Segmenter {
    #g;
    constructor(locales, options) {
      canonicalize(locales);
      this.#g = option(toOptions(options), 'granularity', ['grapheme', 'word', 'sentence'], 'grapheme');
    }
    segment(input) {
      const s = String(input), g = this.#g;
      const segs = [];
      if (g === 'grapheme') {
        // A base character with the marks, joiners, variation selectors
        // and skin tones that attach to it; flag pairs stay together
        const re = /(?:\p{RI}\p{RI}|\r\n|(?:[^\p{M}‍️\u{1F3FB}-\u{1F3FF}])(?:[\p{M}️\u{1F3FB}-\u{1F3FF}]|‍[^\p{M}])*|[\s\S])/gu;
        for (const m of s.matchAll(re)) segs.push({ segment: m[0], index: m.index, input: s });
      } else if (g === 'word') {
        const re = /[\p{L}\p{N}_'’]+|\s+|[^\p{L}\p{N}_\s]/gu;
        for (const m of s.matchAll(re)) segs.push({ segment: m[0], index: m.index, input: s, isWordLike: /[\p{L}\p{N}]/u.test(m[0]) });
      } else {
        const re = /[^.!?]+(?:[.!?]+["')\]]*\s*|$)|[.!?]+\s*/g;
        for (const m of s.matchAll(re)) if (m[0]) segs.push({ segment: m[0], index: m.index, input: s });
      }
      const result = {
        containing(i) { i = Math.trunc(Number(i) || 0); return segs.find((x) => i >= x.index && i < x.index + x.segment.length); },
        [Symbol.iterator]() { return segs[Symbol.iterator](); },
      };
      return result;
    }
    resolvedOptions() { return { locale: LOCALE, granularity: this.#g }; }
    static supportedLocalesOf(locales) { return canonicalize(locales).filter((l) => l.startsWith('en')); }
  }

  // ---- DisplayNames and Locale ----

  const LANGUAGES = { en: 'English', es: 'Spanish', fr: 'French', de: 'German', it: 'Italian', pt: 'Portuguese', ja: 'Japanese', zh: 'Chinese', ko: 'Korean', ru: 'Russian', ar: 'Arabic', hi: 'Hindi', nl: 'Dutch', sv: 'Swedish', pl: 'Polish', tr: 'Turkish' };
  const REGIONS = { US: 'United States', GB: 'United Kingdom', ES: 'Spain', FR: 'France', DE: 'Germany', IT: 'Italy', JP: 'Japan', CN: 'China', IN: 'India', BR: 'Brazil', MX: 'Mexico', CA: 'Canada', AU: 'Australia', RU: 'Russia', KR: 'South Korea' };
  class DisplayNames {
    #o;
    constructor(locales, options) {
      canonicalize(locales);
      options = toOptions(options);
      this.#o = { locale: LOCALE, type: option(options, 'type', ['language', 'region', 'script', 'currency', 'calendar', 'dateTimeField'], undefined), style: option(options, 'style', ['long', 'short', 'narrow'], 'long'), fallback: option(options, 'fallback', ['code', 'none'], 'code') };
      if (!this.#o.type) throw new TypeError('Required option type is missing');
    }
    of(code) {
      code = String(code);
      const { type, fallback } = this.#o;
      const name = type === 'language' ? LANGUAGES[code.split('-')[0].toLowerCase()] : type === 'region' ? REGIONS[code.toUpperCase()] : type === 'currency' ? { USD: 'US Dollar', EUR: 'Euro', GBP: 'British Pound', JPY: 'Japanese Yen' }[code.toUpperCase()] : undefined;
      return name ?? (fallback === 'code' ? code : undefined);
    }
    resolvedOptions() { return { ...this.#o }; }
    static supportedLocalesOf(locales) { return canonicalize(locales).filter((l) => l.startsWith('en')); }
  }
  class Locale {
    constructor(tag, options = {}) {
      const [canon] = canonicalize(tag);
      const parts = canon.split('-');
      this.language = options.language || parts[0];
      this.script = options.script || parts.find((p, i) => i > 0 && p.length === 4);
      this.region = options.region || parts.find((p, i) => i > 0 && /^([A-Z]{2}|\d{3})$/.test(p));
      this.baseName = [this.language, this.script, this.region].filter(Boolean).join('-');
    }
    toString() { return this.baseName; }
    maximize() { return this; }
    minimize() { return this; }
  }

  const Intl = {
    getCanonicalLocales(locales) { return [...new Set(canonicalize(locales))]; },
    supportedValuesOf(key) {
      const values = { calendar: ['gregory'], collation: ['default'], currency: Object.keys(CURRENCY).sort(), numberingSystem: ['latn'], timeZone: ['UTC'], unit: Object.keys(UNITS).sort() }[key];
      if (!values) throw new RangeError(`Invalid key : ${key}`);
      return values;
    },
    NumberFormat, DateTimeFormat, PluralRules, RelativeTimeFormat, ListFormat, Collator, Segmenter, DisplayNames, Locale,
  };
  for (const k of Object.keys(Intl)) Object.defineProperty(Intl, k, { enumerable: false });
  Object.defineProperty(Intl, Symbol.toStringTag, { value: 'Intl', configurable: true });
  return Intl;
})()
