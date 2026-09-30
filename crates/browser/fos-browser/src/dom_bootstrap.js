// Web platform pieces written on top of the native DOM bindings
// (dom_bindings.rs). Runs once per page, before the page's scripts.
(function (global) {
  'use strict';
  const define = (o, props) => {
    for (const k of Object.keys(props)) {
      const d = Object.getOwnPropertyDescriptor(props, k);
      d.enumerable = false;
      Object.defineProperty(o, k, d);
    }
  };
  const E = HTMLElement.prototype;

  // ---- events ----

  class Event {
    constructor(type, init = {}) {
      this.type = String(type);
      this.bubbles = !!init.bubbles;
      this.cancelable = !!init.cancelable;
      this.composed = !!init.composed;
      this.defaultPrevented = false;
      this.target = null;
      this.currentTarget = null;
      this.eventPhase = 0;
      this.timeStamp = performance.now();
      this.isTrusted = false;
      this._stop = false;
      this._stopNow = false;
    }
    preventDefault() { if (this.cancelable) this.defaultPrevented = true; }
    stopPropagation() { this._stop = true; }
    stopImmediatePropagation() { this._stop = true; this._stopNow = true; }
    get returnValue() { return !this.defaultPrevented; }
    set returnValue(v) { if (!v) this.preventDefault(); }
    composedPath() { return this._path ? this._path.slice() : []; }
  }
  Event.NONE = 0; Event.CAPTURING_PHASE = 1; Event.AT_TARGET = 2; Event.BUBBLING_PHASE = 3;
  class CustomEvent extends Event {
    constructor(type, init = {}) { super(type, init); this.detail = init.detail === undefined ? null : init.detail; }
  }
  class UIEvent extends Event {
    constructor(type, init = {}) { super(type, init); this.detail = init.detail || 0; this.view = init.view || null; }
  }
  class MouseEvent extends UIEvent {
    constructor(type, init = {}) {
      super(type, init);
      for (const k of ['screenX', 'screenY', 'clientX', 'clientY', 'button', 'buttons']) this[k] = init[k] || 0;
      for (const k of ['ctrlKey', 'shiftKey', 'altKey', 'metaKey']) this[k] = !!init[k];
      this.relatedTarget = init.relatedTarget || null;
      this.pageX = this.clientX; this.pageY = this.clientY;
    }
  }
  class KeyboardEvent extends UIEvent {
    constructor(type, init = {}) {
      super(type, init);
      this.key = init.key || ''; this.code = init.code || '';
      this.repeat = !!init.repeat;
      for (const k of ['ctrlKey', 'shiftKey', 'altKey', 'metaKey']) this[k] = !!init[k];
    }
  }
  class FocusEvent extends UIEvent {}
  class InputEvent extends UIEvent {}
  class ErrorEvent extends Event {
    constructor(type, init = {}) { super(type, init); this.message = init.message || ''; this.error = init.error; }
  }

  // Listeners of each target, by type: [{callback, capture, once, passive}]
  const listeners = new WeakMap();
  const EventTargetProto = EventTarget.prototype;
  const eventTargetMethods = {
    addEventListener(type, callback, options) {
      if (callback == null) return;
      const capture = typeof options === 'boolean' ? options : !!(options && options.capture);
      const once = !!(options && typeof options === 'object' && options.once);
      let byType = listeners.get(this);
      if (!byType) listeners.set(this, byType = new Map());
      let list = byType.get(type);
      if (!list) byType.set(type, list = []);
      if (list.some(l => l.callback === callback && l.capture === capture)) return;
      list.push({ callback, capture, once });
      const signal = options && typeof options === 'object' && options.signal;
      if (signal) signal.addEventListener('abort', () => this.removeEventListener(type, callback, options));
    },
    removeEventListener(type, callback, options) {
      const capture = typeof options === 'boolean' ? options : !!(options && options.capture);
      const list = listeners.get(this)?.get(type);
      if (!list) return;
      const i = list.findIndex(l => l.callback === callback && l.capture === capture);
      if (i >= 0) { list[i].removed = true; list.splice(i, 1); }
    },
    dispatchEvent(event) {
      event.target = this;
      event.isTrusted = !!event.isTrusted;
      const path = [];
      for (let n = this; n; n = n === document ? global : n.parentNode) {
        path.push(n);
        if (n === global) break;
      }
      event._path = path;
      const invoke = (target, phase) => {
        event.currentTarget = target;
        event.eventPhase = phase;
        let list = listeners.get(target)?.get(event.type);
        if (list) {
          for (const l of list.slice()) {
            if (l.removed) continue;
            if (phase === 1 && !l.capture) continue;
            if (phase === 3 && l.capture) continue;
            if (l.once) target.removeEventListener(event.type, l.callback, l.capture);
            try {
              if (typeof l.callback === 'function') l.callback.call(target, event);
              else if (l.callback && typeof l.callback.handleEvent === 'function') l.callback.handleEvent(event);
            } catch (e) { reportError(e); }
            if (event._stopNow) break;
          }
        }
        // on<type> handler properties and attributes
        if (phase !== 1) {
          const h = handlerOf(target, event.type);
          if (h) {
            try { if (h.call(target, event) === false) event.preventDefault(); } catch (e) { reportError(e); }
          }
        }
      };
      for (let i = path.length - 1; i > 0 && !event._stop; i--) invoke(path[i], 1);
      if (!event._stop) invoke(this, 2);
      if (event.bubbles) for (let i = 1; i < path.length && !event._stop; i++) invoke(path[i], 3);
      event.currentTarget = null;
      event.eventPhase = 0;
      return !event.defaultPrevented;
    },
  };
  define(EventTargetProto, eventTargetMethods);
  // The global object is the window, an EventTarget too
  define(global, eventTargetMethods);

  // Handlers set as `el.onclick = f` or `<div onclick="...">`
  const handlerProps = new WeakMap();
  const compiled = new WeakMap();
  function handlerOf(target, type) {
    const own = handlerProps.get(target)?.[type];
    if (own !== undefined) return typeof own === 'function' ? own : null;
    if (target && typeof target.getAttribute === 'function') {
      const src = target.getAttribute('on' + type);
      if (src) {
        let cache = compiled.get(target);
        if (!cache) compiled.set(target, cache = {});
        if (!cache[type] || cache[type].src !== src) {
          cache[type] = { src, fn: new Function('event', 'with (this) { ' + src + '\n}') };
        }
        return cache[type].fn;
      }
    }
    return null;
  }
  const eventTypes = ['click', 'dblclick', 'mousedown', 'mouseup', 'mousemove', 'mouseover', 'mouseout',
    'mouseenter', 'mouseleave', 'contextmenu', 'wheel', 'keydown', 'keyup', 'keypress', 'input', 'change',
    'submit', 'reset', 'focus', 'blur', 'load', 'error', 'scroll', 'resize', 'touchstart', 'touchend',
    'touchmove', 'pointerdown', 'pointerup', 'pointermove', 'animationend', 'transitionend', 'select',
    'DOMContentLoaded', 'beforeunload', 'unload', 'hashchange', 'popstate', 'message', 'toggle'];
  for (const type of eventTypes) {
    const desc = {
      get() { return handlerProps.get(this)?.[type] ?? null; },
      set(v) {
        let o = handlerProps.get(this);
        if (!o) handlerProps.set(this, o = {});
        o[type] = typeof v === 'function' ? v : null;
      },
      configurable: true,
    };
    Object.defineProperty(EventTargetProto, 'on' + type.toLowerCase(), desc);
    Object.defineProperty(global, 'on' + type.toLowerCase(), desc);
  }

  function reportError(e) {
    console.error('Uncaught', e && e.stack ? e.stack : e);
  }

  // Called by the browser to deliver user input and lifecycle events
  define(global, {
    __fosDispatch(target, type, init) {
      const Ctor = /^(click|dblclick|mouse|contextmenu)/.test(type) ? MouseEvent
        : /^key/.test(type) ? KeyboardEvent : Event;
      const ev = new Ctor(type, init || { bubbles: true, cancelable: true });
      ev.isTrusted = true;
      const notCanceled = target.dispatchEvent(ev);
      if (notCanceled && type === 'click') {
        const a = target.closest && target.closest('a[href]');
        if (a) return 'navigate:' + a.href;
      }
      return notCanceled;
    },
  });

  // ---- element conveniences ----

  // Reflected attributes: string-valued ...
  for (const [prop, attr] of [['title', 'title'], ['lang', 'lang'], ['dir', 'dir'], ['name', 'name'],
    ['type', 'type'], ['alt', 'alt'], ['rel', 'rel'], ['target', 'target'], ['placeholder', 'placeholder'],
    ['htmlFor', 'for'], ['accessKey', 'accesskey'], ['role', 'role'], ['slot', 'slot'],
    ['width', 'width'], ['height', 'height'], ['min', 'min'], ['max', 'max'], ['step', 'step'],
    ['pattern', 'pattern'], ['autocomplete', 'autocomplete'], ['method', 'method'], ['enctype', 'enctype'],
    ['content', 'content'], ['charset', 'charset'], ['media', 'media'], ['label', 'label']]) {
    Object.defineProperty(E, prop, {
      get() { return this.getAttribute(attr) ?? ''; },
      set(v) { this.setAttribute(attr, String(v)); },
      configurable: true,
    });
  }
  // ... URLs, resolved against the document ...
  for (const prop of ['href', 'src', 'action', 'poster', 'cite', 'data']) {
    Object.defineProperty(E, prop, {
      get() {
        const v = this.getAttribute(prop);
        return v === null ? '' : __fosResolveURL(v, document.baseURI);
      },
      set(v) { this.setAttribute(prop, String(v)); },
      configurable: true,
    });
  }
  // ... and boolean
  for (const prop of ['hidden', 'disabled', 'checked', 'selected', 'readOnly', 'required', 'multiple',
    'autofocus', 'async', 'defer', 'noValidate', 'open', 'controls', 'autoplay', 'loop', 'muted']) {
    const attr = prop.toLowerCase();
    Object.defineProperty(E, prop, {
      get() { return this.hasAttribute(attr); },
      set(v) { if (v) this.setAttribute(attr, ''); else this.removeAttribute(attr); },
      configurable: true,
    });
  }
  // Form control values live in the `value` attribute (no separate dirty value)
  Object.defineProperty(E, 'value', {
    get() {
      const tag = this.localName;
      if (tag === 'textarea') return this.textContent;
      if (tag === 'select') {
        const opt = this.querySelector('option[selected]') || this.querySelector('option');
        return opt ? opt.value : '';
      }
      if (tag === 'option') return this.getAttribute('value') ?? this.textContent.trim();
      return this.getAttribute('value') ?? (this.type === 'checkbox' || this.type === 'radio' ? 'on' : '');
    },
    set(v) {
      if (this.localName === 'textarea') this.textContent = String(v);
      else if (this.localName === 'select') {
        for (const o of this.querySelectorAll('option')) o.selected = o.value === String(v);
      } else this.setAttribute('value', v == null ? '' : String(v));
    },
    configurable: true,
  });
  Object.defineProperty(E, 'tabIndex', {
    get() { const v = parseInt(this.getAttribute('tabindex'), 10); return isNaN(v) ? -1 : v; },
    set(v) { this.setAttribute('tabindex', String(v)); },
    configurable: true,
  });
  define(E, {
    get innerText() { return this.textContent; },
    set innerText(v) { this.textContent = v; },
    get outerText() { return this.textContent; },
    toggleAttribute(name, force) {
      const has = this.hasAttribute(name);
      const want = force === undefined ? !has : !!force;
      if (want && !has) this.setAttribute(name, '');
      if (!want && has) this.removeAttribute(name);
      return want;
    },
    get attributes() {
      const list = this.getAttributeNames().map(name => ({ name, localName: name, value: this.getAttribute(name), specified: true }));
      list.getNamedItem = name => list.find(a => a.name === String(name).toLowerCase()) || null;
      list.item = i => list[i] || null;
      return list;
    },
    insertAdjacentElement(where, el) {
      switch (String(where).toLowerCase()) {
        case 'beforebegin': this.before(el); break;
        case 'afterbegin': this.prepend(el); break;
        case 'beforeend': this.append(el); break;
        case 'afterend': this.after(el); break;
        default: throw new SyntaxError('Invalid position');
      }
      return el;
    },
    insertAdjacentText(where, text) { this.insertAdjacentElement(where, document.createTextNode(text)); },
    getBoundingClientRect() {
      return { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0, toJSON() { return this; } };
    },
    getClientRects() { return []; },
    get offsetWidth() { return 0; }, get offsetHeight() { return 0; },
    get offsetTop() { return 0; }, get offsetLeft() { return 0; }, get offsetParent() { return this.parentElement; },
    get clientWidth() { return 0; }, get clientHeight() { return 0; },
    get scrollWidth() { return 0; }, get scrollHeight() { return 0; },
    scrollTop: 0, scrollLeft: 0,
    scrollIntoView() {}, scrollTo() {}, scrollBy() {},
    focus() { activeElement = this; this.dispatchEvent(new FocusEvent('focus')); },
    blur() { if (activeElement === this) activeElement = null; this.dispatchEvent(new FocusEvent('blur')); },
    click() { this.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true })); },
    attachShadow() { return this; },
    animate() { return { finished: Promise.resolve(), cancel() {}, play() {}, pause() {} }; },
  });
  let activeElement = null;

  // classList
  const tokenLists = new WeakMap();
  class DOMTokenList {
    constructor(el, attr) { this._el = el; this._attr = attr; }
    _get() { return (this._el.getAttribute(this._attr) || '').split(/\s+/).filter(Boolean); }
    _set(list) { this._el.setAttribute(this._attr, list.join(' ')); }
    get length() { return this._get().length; }
    get value() { return this._el.getAttribute(this._attr) || ''; }
    set value(v) { this._el.setAttribute(this._attr, v); }
    item(i) { return this._get()[i] ?? null; }
    contains(t) { return this._get().includes(String(t)); }
    add(...tokens) { const l = this._get(); for (const t of tokens) if (!l.includes(t)) l.push(String(t)); this._set(l); }
    remove(...tokens) { const ts = tokens.map(String); this._set(this._get().filter(c => !ts.includes(c))); }
    toggle(t, force) {
      t = String(t);
      const has = this.contains(t);
      const want = force === undefined ? !has : !!force;
      if (want && !has) this.add(t);
      if (!want && has) this.remove(t);
      return want;
    }
    replace(a, b) {
      const l = this._get(); const i = l.indexOf(String(a));
      if (i < 0) return false;
      l[i] = String(b); this._set(l); return true;
    }
    supports() { return true; }
    forEach(f, thisArg) { this._get().forEach((t, i) => f.call(thisArg, t, i, this)); }
    toString() { return this.value; }
    [Symbol.iterator]() { return this._get()[Symbol.iterator](); }
  }
  Object.defineProperty(E, 'classList', {
    get() {
      let l = tokenLists.get(this);
      if (!l) tokenLists.set(this, l = new DOMTokenList(this, 'class'));
      return l;
    },
    set(v) { this.className = v; },
    configurable: true,
  });

  // style: a CSSStyleDeclaration over the `style` attribute
  const camelToKebab = s => s.startsWith('--') ? s : s.replace(/[A-Z]/g, c => '-' + c.toLowerCase()).replace(/^(webkit|moz|ms)-/, '-$1-');
  function parseStyle(text) {
    const map = new Map();
    for (const decl of (text || '').split(';')) {
      const i = decl.indexOf(':');
      if (i < 0) continue;
      const k = decl.slice(0, i).trim().toLowerCase();
      if (k) map.set(k, decl.slice(i + 1).trim());
    }
    return map;
  }
  const writeStyle = (el, map) => {
    const text = [...map].map(([k, v]) => k + ': ' + v).join('; ');
    if (text) el.setAttribute('style', text + ';'); else el.removeAttribute('style');
  };
  const styleMethods = {
    getPropertyValue(el, name) { return parseStyle(el.getAttribute('style')).get(String(name).toLowerCase()) ?? ''; },
    setProperty(el, name, value) {
      const map = parseStyle(el.getAttribute('style'));
      name = String(name).toLowerCase();
      if (value === null || value === undefined || value === '') map.delete(name);
      else map.set(name, String(value).replace(/\s*!important\s*$/, ''));
      writeStyle(el, map);
    },
    removeProperty(el, name) {
      const map = parseStyle(el.getAttribute('style'));
      const old = map.get(String(name).toLowerCase()) ?? '';
      map.delete(String(name).toLowerCase());
      writeStyle(el, map);
      return old;
    },
  };
  const styles = new WeakMap();
  Object.defineProperty(E, 'style', {
    get() {
      let s = styles.get(this);
      if (s) return s;
      const el = this;
      s = new Proxy({}, {
        get(_, key) {
          if (typeof key === 'symbol') return undefined;
          if (key in styleMethods) return (...a) => styleMethods[key](el, ...a);
          if (key === 'cssText') return el.getAttribute('style') || '';
          if (key === 'length') return parseStyle(el.getAttribute('style')).size;
          if (/^\d+$/.test(key)) return [...parseStyle(el.getAttribute('style')).keys()][+key];
          if (key === 'item') return i => [...parseStyle(el.getAttribute('style')).keys()][i] ?? '';
          if (key === 'cssFloat') key = 'float';
          return parseStyle(el.getAttribute('style')).get(camelToKebab(key)) ?? '';
        },
        set(_, key, value) {
          if (typeof key === 'symbol') return true;
          if (key === 'cssText') { el.setAttribute('style', String(value)); return true; }
          if (key === 'cssFloat') key = 'float';
          styleMethods.setProperty(el, camelToKebab(key), value);
          return true;
        },
        has(_, key) { return typeof key === 'string'; },
      });
      styles.set(this, s);
      return s;
    },
    set(v) { this.setAttribute('style', String(v)); },
    configurable: true,
  });

  // dataset: data-* attributes
  const datasets = new WeakMap();
  const dataAttr = key => 'data-' + String(key).replace(/[A-Z]/g, c => '-' + c.toLowerCase());
  Object.defineProperty(E, 'dataset', {
    get() {
      let d = datasets.get(this);
      if (d) return d;
      const el = this;
      d = new Proxy({}, {
        get(_, key) { return typeof key === 'string' ? el.getAttribute(dataAttr(key)) ?? undefined : undefined; },
        set(_, key, v) { el.setAttribute(dataAttr(key), String(v)); return true; },
        has(_, key) { return typeof key === 'string' && el.hasAttribute(dataAttr(key)); },
        deleteProperty(_, key) { el.removeAttribute(dataAttr(key)); return true; },
        ownKeys() {
          return el.getAttributeNames().filter(n => n.startsWith('data-'))
            .map(n => n.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase()));
        },
        getOwnPropertyDescriptor(_, key) {
          const v = el.getAttribute(dataAttr(key));
          return v === null ? undefined : { value: v, writable: true, enumerable: true, configurable: true };
        },
      });
      datasets.set(this, d);
      return d;
    },
    configurable: true,
  });

  // NodeList helpers (query results are arrays with NodeList.prototype)
  define(NodeList.prototype, { item(i) { return this[i] ?? null; } });
  global.HTMLCollection = NodeList;

  // Tag-specific interfaces, for `instanceof` checks; all share HTMLElement.prototype
  for (const name of ['HTMLDivElement', 'HTMLSpanElement', 'HTMLAnchorElement', 'HTMLImageElement',
    'HTMLInputElement', 'HTMLButtonElement', 'HTMLFormElement', 'HTMLSelectElement', 'HTMLOptionElement',
    'HTMLTextAreaElement', 'HTMLScriptElement', 'HTMLStyleElement', 'HTMLLinkElement', 'HTMLCanvasElement',
    'HTMLVideoElement', 'HTMLAudioElement', 'HTMLMediaElement', 'HTMLIFrameElement', 'HTMLTemplateElement',
    'HTMLParagraphElement', 'HTMLHeadingElement', 'HTMLLIElement', 'HTMLUListElement', 'HTMLTableElement',
    'HTMLBodyElement', 'HTMLHeadElement', 'HTMLHtmlElement', 'HTMLLabelElement', 'HTMLUnknownElement',
    'SVGElement', 'SVGSVGElement']) {
    global[name] = HTMLElement;
  }

  // ---- document ----

  let readyState = 'loading';
  define(Document.prototype, {
    get readyState() { return readyState; },
    get defaultView() { return global; },
    get activeElement() { return activeElement || this.body; },
    get baseURI() {
      const base = this.querySelector('base[href]');
      return base ? __fosResolveURL(base.getAttribute('href'), this.URL) : this.URL;
    },
    get location() { return global.location; },
    set location(v) { global.location.href = v; },
    get cookie() { return cookieJar; },
    set cookie(v) {
      const [pair] = String(v).split(';');
      const i = pair.indexOf('=');
      if (i < 0) return;
      const jar = new Map(cookieJar ? cookieJar.split('; ').map(c => { const j = c.indexOf('='); return [c.slice(0, j), c.slice(j + 1)]; }) : []);
      jar.set(pair.slice(0, i).trim(), pair.slice(i + 1).trim());
      cookieJar = [...jar].map(([k, v]) => k + '=' + v).join('; ');
    },
    get forms() { return this.querySelectorAll('form'); },
    get images() { return this.querySelectorAll('img'); },
    get links() { return this.querySelectorAll('a[href], area[href]'); },
    get scripts() { return this.querySelectorAll('script'); },
    get characterSet() { return 'UTF-8'; },
    get charset() { return 'UTF-8'; },
    get compatMode() { return 'CSS1Compat'; },
    get contentType() { return 'text/html'; },
    get visibilityState() { return 'visible'; },
    get hidden() { return false; },
    get referrer() { return ''; },
    get domain() { return global.location.hostname; },
    get currentScript() { return currentScript; },
    get implementation() { return implementation; },
    getElementsByName(name) { return this.querySelectorAll('[name="' + String(name).replace(/"/g, '\\"') + '"]'); },
    hasFocus() { return true; },
    createEvent(kind) {
      const ev = new Event('');
      ev.initEvent = function (type, bubbles, cancelable) { this.type = type; this.bubbles = !!bubbles; this.cancelable = !!cancelable; };
      ev.initCustomEvent = function (type, bubbles, cancelable, detail) { this.initEvent(type, bubbles, cancelable); this.detail = detail; };
      return ev;
    },
    createRange() {
      return { setStart() {}, setEnd() {}, collapse() {}, selectNodeContents() {}, getBoundingClientRect: () => E.getBoundingClientRect(),
        createContextualFragment(html) { const t = document.createElement('div'); t.innerHTML = html; const f = document.createDocumentFragment(); f.append(...t.childNodes); return f; } };
    },
    createTreeWalker(root) { return { currentNode: root, nextNode: () => null }; },
    write(...parts) { writeBuffer.push(parts.join('')); },
    writeln(...parts) { writeBuffer.push(parts.join('') + '\n'); },
    open() {}, close() {},
    execCommand() { return false; },
    elementFromPoint() { return null; },
  });
  // Documents made by scripts: a detached <html> tree standing in for the
  // new document, sharing the page's node arena
  function createHTMLDocument(title) {
    const html = document.createElement('html');
    const head = html.appendChild(document.createElement('head'));
    const body = html.appendChild(document.createElement('body'));
    if (title !== undefined) head.appendChild(document.createElement('title')).textContent = String(title);
    const doc = Object.create(document);
    define(doc, {
      documentElement: html, head, body,
      get title() { const t = head.querySelector('title'); return t ? t.textContent : ''; },
      getElementById(id) { return html.querySelector('#' + CSS.escape(String(id))); },
      querySelector(s) { return html.matches(s) ? html : html.querySelector(s); },
      querySelectorAll(s) { return html.querySelectorAll(s); },
      getElementsByTagName(t) { return html.getElementsByTagName(t); },
      getElementsByClassName(c) { return html.getElementsByClassName(c); },
      get readyState() { return 'complete'; },
      get defaultView() { return null; },
    });
    return doc;
  }
  const implementation = {
    hasFeature: () => true,
    createHTMLDocument,
    createDocument: () => createHTMLDocument(),
  };
  global.CSS = {
    escape(s) { return String(s).replace(/([\0-\x1f\x7f]|^-?\d)|^-$|[^\0-\x1f\x7f-\uFFFF\w-]/g, (m, ctl) => ctl ? (m === '\0' ? '\uFFFD' : m.slice(0, -1) + '\\' + m.slice(-1).charCodeAt(0).toString(16) + ' ') : '\\' + m); },
    supports: () => false,
  };
  let cookieJar = '';
  let currentScript = null;
  // document.write output, inserted after the running script by the browser
  const writeBuffer = [];

  // ---- window ----

  class URLSearchParams {
    constructor(init) {
      this._list = [];
      if (init == null) return;
      if (typeof init === 'object') {
        const entries = Symbol.iterator in init ? init : Object.entries(init);
        for (const [k, v] of entries) this._list.push([String(k), String(v)]);
        return;
      }
      let s = String(init);
      if (s.startsWith('?')) s = s.slice(1);
      for (const part of s.split('&')) {
        if (!part) continue;
        const i = part.indexOf('=');
        const dec = x => { try { return decodeURIComponent(x.replace(/\+/g, ' ')); } catch { return x; } };
        this._list.push(i < 0 ? [dec(part), ''] : [dec(part.slice(0, i)), dec(part.slice(i + 1))]);
      }
    }
    append(k, v) { this._list.push([String(k), String(v)]); this._update(); }
    delete(k) { this._list = this._list.filter(([n]) => n !== String(k)); this._update(); }
    get(k) { const e = this._list.find(([n]) => n === String(k)); return e ? e[1] : null; }
    getAll(k) { return this._list.filter(([n]) => n === String(k)).map(e => e[1]); }
    has(k) { return this._list.some(([n]) => n === String(k)); }
    set(k, v) {
      k = String(k);
      const i = this._list.findIndex(([n]) => n === k);
      if (i < 0) this._list.push([k, String(v)]);
      else { this._list[i][1] = String(v); this._list = this._list.filter(([n], j) => n !== k || j === i); }
      this._update();
    }
    sort() { this._list.sort((a, b) => a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0); this._update(); }
    forEach(f, thisArg) { for (const [k, v] of this._list) f.call(thisArg, v, k, this); }
    keys() { return this._list.map(e => e[0])[Symbol.iterator](); }
    values() { return this._list.map(e => e[1])[Symbol.iterator](); }
    entries() { return this._list.map(e => [e[0], e[1]])[Symbol.iterator](); }
    [Symbol.iterator]() { return this.entries(); }
    get size() { return this._list.length; }
    toString() {
      const enc = s => encodeURIComponent(s).replace(/%20/g, '+');
      return this._list.map(([k, v]) => enc(k) + '=' + enc(v)).join('&');
    }
    _update() { if (this._url) this._url._search = this._list.length ? '?' + this.toString() : ''; }
  }

  const URL_RE = /^([a-zA-Z][a-zA-Z0-9+.-]*:)(?:\/\/(?:([^:@\/]*)(?::([^@\/]*))?@)?([^:\/?#]*)(?::(\d*))?)?([^?#]*)(\?[^#]*)?(#.*)?$/;
  const DEFAULT_PORTS = { 'http:': '80', 'https:': '443', 'ws:': '80', 'wss:': '443', 'ftp:': '21' };
  class URL {
    constructor(url, base) {
      url = String(url);
      const abs = base === undefined ? url : __fosResolveURL(url, String(base instanceof URL ? base.href : base));
      const m = URL_RE.exec(abs);
      if (!m) throw new TypeError("Failed to construct 'URL': Invalid URL");
      this._protocol = m[1].toLowerCase();
      this.username = m[2] || '';
      this.password = m[3] || '';
      this._hostname = (m[4] || '').toLowerCase();
      this._port = m[5] && m[5] !== DEFAULT_PORTS[this._protocol] ? m[5] : '';
      this._pathname = m[6] || (m[4] !== undefined ? '/' : '');
      this._search = m[7] && m[7].length > 1 ? m[7] : '';
      this.hash = m[8] && m[8].length > 1 ? m[8] : '';
      this._params = null;
    }
    get protocol() { return this._protocol; }
    set protocol(v) { v = String(v); this._protocol = (v.endsWith(':') ? v : v + ':').toLowerCase(); }
    get hostname() { return this._hostname; }
    set hostname(v) { this._hostname = String(v).toLowerCase(); }
    get port() { return this._port; }
    set port(v) { this._port = String(v) === DEFAULT_PORTS[this._protocol] ? '' : String(v); }
    get host() { return this._hostname + (this._port ? ':' + this._port : ''); }
    set host(v) { const [h, p] = String(v).split(':'); this.hostname = h; this.port = p || ''; }
    get origin() { return this._hostname ? this._protocol + '//' + this.host : 'null'; }
    get pathname() { return this._pathname; }
    set pathname(v) { v = String(v); this._pathname = v.startsWith('/') ? v : '/' + v; }
    get search() { return this._search; }
    set search(v) { v = String(v); this._search = v && v !== '?' ? (v.startsWith('?') ? v : '?' + v) : ''; this._params = null; }
    get searchParams() {
      if (!this._params) { this._params = new URLSearchParams(this._search); this._params._url = this; }
      return this._params;
    }
    get href() {
      const auth = this.username ? this.username + (this.password ? ':' + this.password : '') + '@' : '';
      const slashes = this._hostname || this._protocol === 'file:' ? '//' : '';
      return this._protocol + slashes + auth + this.host + this._pathname + this._search + this.hash;
    }
    set href(v) { Object.assign(this, new URL(v)); }
    toString() { return this.href; }
    toJSON() { return this.href; }
    static canParse(u, b) { try { new URL(u, b); return true; } catch { return false; } }
    static createObjectURL() { return 'blob:' + global.location.origin + '/' + Math.random().toString(36).slice(2); }
    static revokeObjectURL() {}
  }

  // location: a URL whose navigation requests go to the browser
  class Location {
    constructor() { this._url = new URL(document.URL); }
    get href() { return this._url.href; }
    set href(v) { __fosNavigate(new URL(v, this._url.href).href); }
    assign(v) { this.href = v; }
    replace(v) { this.href = v; }
    reload() { __fosNavigate(this._url.href); }
    toString() { return this.href; }
    get hash() { return this._url.hash; }
    set hash(v) {
      v = String(v);
      const old = this._url.href;
      this._url.hash = v && v !== '#' ? (v.startsWith('#') ? v : '#' + v) : '';
      if (old !== this._url.href) global.dispatchEvent(new Event('hashchange'));
    }
  }
  for (const k of ['protocol', 'host', 'hostname', 'port', 'pathname', 'search', 'origin']) {
    Object.defineProperty(Location.prototype, k, { get() { return this._url[k]; }, set(v) { const u = new URL(this._url.href); u[k] = v; this.href = u.href; } });
  }
  const pendingNavigation = [];
  function __fosNavigate(url) { pendingNavigation.push(url); }

  class Storage {
    constructor() { Object.defineProperty(this, '_m', { value: new Map() }); }
    get length() { return this._m.size; }
    key(i) { return [...this._m.keys()][i] ?? null; }
    getItem(k) { return this._m.has(String(k)) ? this._m.get(String(k)) : null; }
    setItem(k, v) { this._m.set(String(k), String(v)); }
    removeItem(k) { this._m.delete(String(k)); }
    clear() { this._m.clear(); }
  }

  function makeSignal() {
    const s = Object.create(EventTarget.prototype);
    s.aborted = false;
    s.reason = undefined;
    s.throwIfAborted = function () { if (this.aborted) throw this.reason; };
    return s;
  }
  class AbortController {
    constructor() { this.signal = makeSignal(); }
    abort(reason) {
      if (this.signal.aborted) return;
      this.signal.aborted = true;
      this.signal.reason = reason ?? new Error('AbortError');
      this.signal.dispatchEvent(new Event('abort'));
    }
  }

  const noopObserver = class { constructor(cb) { this._cb = cb; } observe() {} unobserve() {} disconnect() {} takeRecords() { return []; } };

  function matchMedia(query) {
    const q = String(query);
    let matches = false;
    const w = global.innerWidth;
    const min = /min-width:\s*(\d+)px/.exec(q), max = /max-width:\s*(\d+)px/.exec(q);
    if (min || max) matches = (!min || w >= +min[1]) && (!max || w <= +max[1]);
    else if (/prefers-color-scheme:\s*light|screen|all/.test(q)) matches = true;
    const mql = Object.create(EventTarget.prototype);
    Object.assign(mql, { matches, media: q, onchange: null, addListener() {}, removeListener() {} });
    return mql;
  }

  let rafId = 0;
  const rafs = new Map();
  define(global, {
    self: global, top: global, parent: global, frames: global, opener: null, closed: false,
    frameElement: null, length: 0, name: '', origin: '', isSecureContext: true,
    innerWidth: 1024, innerHeight: 768, outerWidth: 1024, outerHeight: 768, devicePixelRatio: 1,
    scrollX: 0, scrollY: 0, pageXOffset: 0, pageYOffset: 0, screenX: 0, screenY: 0,
    screen: { width: 1920, height: 1080, availWidth: 1920, availHeight: 1080, colorDepth: 24, pixelDepth: 24 },
    navigator: {
      userAgent: 'Mozilla/5.0 (X11; Linux x86_64) fOS/0.1 (KHTML, like Gecko)',
      appName: 'Netscape', appVersion: '5.0', appCodeName: 'Mozilla', product: 'Gecko',
      platform: 'Linux x86_64', vendor: '', language: 'en-US', languages: ['en-US', 'en'],
      onLine: true, cookieEnabled: true, doNotTrack: null, hardwareConcurrency: 4, maxTouchPoints: 0,
      webdriver: false, javaEnabled: () => false, sendBeacon: () => true,
      clipboard: { writeText: () => Promise.resolve(), readText: () => Promise.resolve('') },
    },
    history: {
      length: 1, state: null, scrollRestoration: 'auto',
      pushState(state) { this.state = state; }, replaceState(state) { this.state = state; },
      back() {}, forward() {}, go() {},
    },
    localStorage: new Storage(),
    sessionStorage: new Storage(),
    Event, CustomEvent, UIEvent, MouseEvent, KeyboardEvent, FocusEvent, InputEvent, ErrorEvent,
    PointerEvent: MouseEvent, TouchEvent: UIEvent, WheelEvent: MouseEvent, AnimationEvent: Event,
    TransitionEvent: Event, PopStateEvent: Event, HashChangeEvent: Event, MessageEvent: Event,
    ProgressEvent: Event,
    URL, URLSearchParams, Storage, DOMTokenList, AbortController,
    AbortSignal: { abort(reason) { const c = new AbortController(); c.abort(reason); return c.signal; }, timeout() { return makeSignal(); } },
    MutationObserver: noopObserver, IntersectionObserver: noopObserver, ResizeObserver: noopObserver,
    PerformanceObserver: noopObserver,
    requestAnimationFrame(cb) {
      const id = ++rafId;
      rafs.set(id, setTimeout(() => { rafs.delete(id); cb(performance.now()); }, 16));
      return id;
    },
    cancelAnimationFrame(id) { clearTimeout(rafs.get(id)); rafs.delete(id); },
    requestIdleCallback(cb) { return setTimeout(() => cb({ didTimeout: false, timeRemaining: () => 50 }), 1); },
    cancelIdleCallback(id) { clearTimeout(id); },
    matchMedia,
    getComputedStyle(el) { return el.style; },
    getSelection() { return { rangeCount: 0, removeAllRanges() {}, addRange() {}, toString: () => '' }; },
    scrollTo() {}, scrollBy() {}, scroll() {}, focus() {}, blur() {}, print() {}, stop() {},
    alert(msg) { console.info('[alert]', msg); },
    confirm(msg) { console.info('[confirm]', msg); return false; },
    prompt(msg) { console.info('[prompt]', msg); return null; },
    open() { return null; }, close() {},
    postMessage(data) { setTimeout(() => global.dispatchEvent(Object.assign(new Event('message'), { data, origin: global.location.origin, source: global })), 0); },
    structuredClone(v) { return v === undefined ? v : JSON.parse(JSON.stringify(v)); },
    reportError,
    fetch() { return Promise.reject(new TypeError('Failed to fetch')); },
    XMLHttpRequest: class XMLHttpRequest {
      constructor() { this.readyState = 0; this.status = 0; this.responseText = ''; this.response = ''; this._h = {}; }
      open(method, url) { this._url = url; this.readyState = 1; }
      setRequestHeader() {} getResponseHeader() { return null; } getAllResponseHeaders() { return ''; }
      overrideMimeType() {} abort() {}
      addEventListener(t, f) { this._h[t] = f; } removeEventListener() {}
      send() {
        setTimeout(() => {
          this.readyState = 4;
          const ev = new Event('error');
          if (this.onreadystatechange) this.onreadystatechange(ev);
          if (this.onerror) this.onerror(ev);
          if (this._h.error) this._h.error(ev);
          if (this.onloadend) this.onloadend(ev);
        }, 0);
      }
    },
    __fosPageState() {
      return { navigate: pendingNavigation.splice(0), written: writeBuffer.splice(0) };
    },
    __fosSetReadyState(s) {
      readyState = s;
      document.dispatchEvent(new Event('readystatechange'));
    },
    __fosSetCurrentScript(s) { currentScript = s; },
  });
  global.location = new Location();
  global.origin = global.location.origin;
})(globalThis);
