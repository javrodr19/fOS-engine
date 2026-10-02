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
  class PopStateEvent extends Event {
    constructor(type, init = {}) { super(type, init); this.state = init.state ?? null; }
  }
  class HashChangeEvent extends Event {
    constructor(type, init = {}) { super(type, init); this.oldURL = String(init.oldURL ?? ''); this.newURL = String(init.newURL ?? ''); }
  }
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
  // Called bare (`addEventListener('load', f)`), they act on the window
  const eventTargetMethods = {
    addEventListener(type, callback, options) {
      if (this == null) return global.addEventListener(type, callback, options);
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
      if (this == null) return global.removeEventListener(type, callback, options);
      const capture = typeof options === 'boolean' ? options : !!(options && options.capture);
      const list = listeners.get(this)?.get(type);
      if (!list) return;
      const i = list.findIndex(l => l.callback === callback && l.capture === capture);
      if (i >= 0) { list[i].removed = true; list.splice(i, 1); }
    },
    dispatchEvent(event) {
      if (this == null) return global.dispatchEvent(event);
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
    'DOMContentLoaded', 'beforeunload', 'unload', 'hashchange', 'popstate', 'message', 'toggle',
    'abort', 'timeout', 'loadstart', 'progress', 'loadend', 'readystatechange'];
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

  // Reflected attributes common to all HTML elements (those of particular
  // elements are defined on their interfaces, below)
  for (const [prop, attr] of [['title', 'title'], ['lang', 'lang'], ['dir', 'dir'], ['accessKey', 'accesskey'],
    ['role', 'role'], ['slot', 'slot'], ['nonce', 'nonce']]) {
    Object.defineProperty(E, prop, {
      get() { return this.getAttribute(attr) ?? ''; },
      set(v) { this.setAttribute(attr, String(v)); },
      configurable: true,
    });
  }
  for (const prop of ['hidden', 'autofocus', 'inert']) {
    Object.defineProperty(E, prop, {
      get() { return this.hasAttribute(prop); },
      set(v) { if (v) this.setAttribute(prop, ''); else this.removeAttribute(prop); },
      configurable: true,
    });
  }
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
    // Geometry from the browser's layout: [x, y, width, height] in
    // document coordinates, or null when the element is not rendered
    getBoundingClientRect() {
      const g = __fosGeometry(this);
      const v = __fosViewport();
      return g ? domRect(g[0] - v[2], g[1] - v[3], g[2], g[3]) : domRect(0, 0, 0, 0);
    },
    getClientRects() { return __fosGeometry(this) ? [this.getBoundingClientRect()] : []; },
    get offsetWidth() { const g = __fosGeometry(this); return g ? Math.round(g[2]) : 0; },
    get offsetHeight() { const g = __fosGeometry(this); return g ? Math.round(g[3]) : 0; },
    get offsetTop() { const g = __fosGeometry(this); return g ? Math.round(g[1]) : 0; },
    get offsetLeft() { const g = __fosGeometry(this); return g ? Math.round(g[0]) : 0; },
    get offsetParent() {
      if (this.localName === 'body' || this.localName === 'html' || !__fosGeometry(this)) return null;
      return document.body;
    },
    get clientWidth() {
      if (this === document.documentElement) return __fosViewport()[0];
      const g = __fosGeometry(this); return g ? Math.round(g[2]) : 0;
    },
    get clientHeight() {
      if (this === document.documentElement) return __fosViewport()[1];
      const g = __fosGeometry(this); return g ? Math.round(g[3]) : 0;
    },
    get clientTop() { return 0; }, get clientLeft() { return 0; },
    get scrollWidth() {
      if (this === document.documentElement || this === document.body) return __fosViewport()[0];
      return this.clientWidth;
    },
    get scrollHeight() {
      if (this === document.documentElement || this === document.body) return Math.round(__fosViewport()[4]);
      return this.clientHeight;
    },
    get scrollTop() { return this === document.documentElement || this === document.body ? __fosViewport()[3] : 0; },
    set scrollTop(v) { if (this === document.documentElement || this === document.body) __fosScrollTo(+v || 0); },
    get scrollLeft() { return 0; },
    set scrollLeft(v) {},
    scrollIntoView() { const g = __fosGeometry(this); if (g) __fosScrollTo(g[1]); },
    scrollTo() {}, scrollBy() {},
    focus() { activeElement = this; this.dispatchEvent(new FocusEvent('focus')); },
    blur() { if (activeElement === this) activeElement = null; this.dispatchEvent(new FocusEvent('blur')); },
    click() { this.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true })); },
    attachShadow() { return this; },
    animate() { return { finished: Promise.resolve(), cancel() {}, play() {}, pause() {} }; },
  });
  let activeElement = null;
  function domRect(x, y, width, height) {
    return { x, y, width, height, left: x, top: y, right: x + width, bottom: y + height,
      toJSON() { return { x, y, width, height, left: x, top: y, right: x + width, bottom: y + height }; } };
  }
  global.DOMRect = class DOMRect {
    constructor(x = 0, y = 0, width = 0, height = 0) { Object.assign(this, domRect(x, y, width, height)); }
  };

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

  // Node comparison (DOM "equals")
  define(Node.prototype, {
    isSameNode(other) { return this === other; },
    isEqualNode(other) {
      if (!other || this.nodeType !== other.nodeType || this.nodeName !== other.nodeName) return false;
      if (this.nodeType === 1) {
        const names = this.getAttributeNames();
        const otherNames = other.getAttributeNames();
        if (names.length !== otherNames.length) return false;
        for (const n of names) {
          if (other.getAttribute(n) !== this.getAttribute(n)) return false;
        }
      } else if (this.nodeType === 3 || this.nodeType === 8) {
        if (this.nodeValue !== other.nodeValue) return false;
      }
      const a = this.childNodes, b = other.childNodes;
      if (a.length !== b.length) return false;
      for (let i = 0; i < a.length; i++) {
        if (!a[i].isEqualNode(b[i])) return false;
      }
      return true;
    },
  });

  // NodeList helpers (query results are arrays with NodeList.prototype)
  define(NodeList.prototype, { item(i) { return this[i] ?? null; } });
  global.HTMLCollection = NodeList;

  // ---- node type constants, document order, traversal (DOM §4.4, §6) ----
  const nodeConstants = {
    ELEMENT_NODE: 1, ATTRIBUTE_NODE: 2, TEXT_NODE: 3, CDATA_SECTION_NODE: 4, ENTITY_REFERENCE_NODE: 5,
    ENTITY_NODE: 6, PROCESSING_INSTRUCTION_NODE: 7, COMMENT_NODE: 8, DOCUMENT_NODE: 9,
    DOCUMENT_TYPE_NODE: 10, DOCUMENT_FRAGMENT_NODE: 11, NOTATION_NODE: 12,
    DOCUMENT_POSITION_DISCONNECTED: 1, DOCUMENT_POSITION_PRECEDING: 2, DOCUMENT_POSITION_FOLLOWING: 4,
    DOCUMENT_POSITION_CONTAINS: 8, DOCUMENT_POSITION_CONTAINED_BY: 16,
    DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC: 32,
  };
  for (const [k, v] of Object.entries(nodeConstants)) {
    Object.defineProperty(Node, k, { value: v, enumerable: true });
    Object.defineProperty(Node.prototype, k, { value: v, enumerable: true });
  }
  const ancestry = n => { const chain = []; for (; n; n = n.parentNode) chain.push(n); return chain.reverse(); };
  define(Node.prototype, {
    compareDocumentPosition(other) {
      if (!(other instanceof Node)) throw new TypeError("Failed to execute 'compareDocumentPosition' on 'Node': parameter 1 is not of type 'Node'.");
      if (this === other) return 0;
      const a = ancestry(other), b = ancestry(this);
      if (a[0] !== b[0]) return 1 | 32 | 2; // disconnected; any consistent order will do
      let i = 0;
      while (i < a.length && i < b.length && a[i] === b[i]) i++;
      if (i === b.length) return 16 | 4;  // other is inside this
      if (i === a.length) return 8 | 2;   // other contains this
      // Siblings under the common ancestor decide the order
      for (let s = a[i].nextSibling; s; s = s.nextSibling) if (s === b[i]) return 2;
      return 4;
    },
  });

  const NodeFilter = {
    FILTER_ACCEPT: 1, FILTER_REJECT: 2, FILTER_SKIP: 3,
    SHOW_ALL: 0xFFFFFFFF, SHOW_ELEMENT: 0x1, SHOW_ATTRIBUTE: 0x2, SHOW_TEXT: 0x4, SHOW_CDATA_SECTION: 0x8,
    SHOW_ENTITY_REFERENCE: 0x10, SHOW_ENTITY: 0x20, SHOW_PROCESSING_INSTRUCTION: 0x40, SHOW_COMMENT: 0x80,
    SHOW_DOCUMENT: 0x100, SHOW_DOCUMENT_TYPE: 0x200, SHOW_DOCUMENT_FRAGMENT: 0x400, SHOW_NOTATION: 0x800,
  };
  // The "filter" algorithm shared by TreeWalker and NodeIterator
  function traversalFilter(t, node) {
    if (t._active) throw new DOMException('Filter is already running', 'InvalidStateError');
    if (!((1 << (node.nodeType - 1)) & t.whatToShow)) return 3;
    const f = t.filter;
    if (f === null) return 1;
    t._active = true;
    try {
      const r = typeof f === 'function' ? f.call(undefined, node) : f.acceptNode(node);
      return Number(r) >>> 0 & 0xFFFF;
    } finally { t._active = false; }
  }
  function traversalArgs(root, whatToShow, filter) {
    if (!(root instanceof Node)) throw new TypeError("parameter 1 is not of type 'Node'.");
    return [root, whatToShow === undefined ? 0xFFFFFFFF : whatToShow >>> 0, filter === undefined ? null : filter];
  }

  class TreeWalker {
    constructor(root, whatToShow, filter) {
      [this.root, this.whatToShow, this.filter] = traversalArgs(root, whatToShow, filter);
      this._current = this.root;
      this._active = false;
    }
    get currentNode() { return this._current; }
    set currentNode(n) {
      if (!(n instanceof Node)) throw new TypeError("Failed to set 'currentNode' on 'TreeWalker': not a Node.");
      this._current = n;
    }
    parentNode() {
      for (let n = this._current; n && n !== this.root;) {
        n = n.parentNode;
        if (n && traversalFilter(this, n) === 1) return (this._current = n);
      }
      return null;
    }
    _children(first) {
      let node = first ? this._current.firstChild : this._current.lastChild;
      while (node) {
        const r = traversalFilter(this, node);
        if (r === 1) return (this._current = node);
        if (r === 3) {
          const child = first ? node.firstChild : node.lastChild;
          if (child) { node = child; continue; }
        }
        while (node) {
          const sibling = first ? node.nextSibling : node.previousSibling;
          if (sibling) { node = sibling; break; }
          const parent = node.parentNode;
          if (!parent || parent === this.root || parent === this._current) return null;
          node = parent;
        }
      }
      return null;
    }
    firstChild() { return this._children(true); }
    lastChild() { return this._children(false); }
    _siblings(next) {
      let node = this._current;
      if (node === this.root) return null;
      for (;;) {
        let sibling = next ? node.nextSibling : node.previousSibling;
        while (sibling) {
          node = sibling;
          const r = traversalFilter(this, node);
          if (r === 1) return (this._current = node);
          sibling = next ? node.firstChild : node.lastChild;
          if (r === 2 || !sibling) sibling = next ? node.nextSibling : node.previousSibling;
        }
        node = node.parentNode;
        if (!node || node === this.root) return null;
        if (traversalFilter(this, node) === 1) return null;
      }
    }
    nextSibling() { return this._siblings(true); }
    previousSibling() { return this._siblings(false); }
    previousNode() {
      let node = this._current;
      while (node !== this.root) {
        let sibling = node.previousSibling;
        while (sibling) {
          node = sibling;
          let r = traversalFilter(this, node);
          while (r !== 2 && node.lastChild) {
            node = node.lastChild;
            r = traversalFilter(this, node);
          }
          if (r === 1) return (this._current = node);
          sibling = node.previousSibling;
        }
        if (node === this.root || !node.parentNode) return null;
        node = node.parentNode;
        if (traversalFilter(this, node) === 1) return (this._current = node);
      }
      return null;
    }
    nextNode() {
      let node = this._current, r = 1;
      for (;;) {
        while (r !== 2 && node.firstChild) {
          node = node.firstChild;
          r = traversalFilter(this, node);
          if (r === 1) return (this._current = node);
        }
        let sibling = null;
        for (let t = node; t; t = t.parentNode) {
          if (t === this.root) return null;
          sibling = t.nextSibling;
          if (sibling) break;
        }
        if (!sibling) return null;
        node = sibling;
        r = traversalFilter(this, node);
        if (r === 1) return (this._current = node);
      }
    }
  }

  // Node iterators that are alive, so removals can move their reference
  // (the "pre-removing steps"); held weakly so dropped iterators cost nothing
  const liveIterators = new Set();
  const following = (node, root) => {
    if (node.firstChild) return node.firstChild;
    for (let n = node; n && n !== root; n = n.parentNode) if (n.nextSibling) return n.nextSibling;
    return null;
  };
  const preceding = (node, root) => {
    if (node === root) return null;
    let p = node.previousSibling;
    if (!p) return node.parentNode;
    while (p.lastChild) p = p.lastChild;
    return p;
  };
  class NodeIterator {
    constructor(root, whatToShow, filter) {
      [this.root, this.whatToShow, this.filter] = traversalArgs(root, whatToShow, filter);
      this._ref = this.root;
      this._before = true;
      this._active = false;
      liveIterators.add(new WeakRef(this));
    }
    get referenceNode() { return this._ref; }
    get pointerBeforeReferenceNode() { return this._before; }
    _traverse(next) {
      let node = this._ref, before = this._before;
      for (;;) {
        if (next) {
          if (!before) { node = following(node, this.root); if (!node) return null; } else before = false;
        } else {
          if (before) { node = preceding(node, this.root); if (!node) return null; } else before = true;
        }
        if (traversalFilter(this, node) === 1) break;
      }
      this._ref = node;
      this._before = before;
      return node;
    }
    nextNode() { return this._traverse(true); }
    previousNode() { return this._traverse(false); }
    detach() {}
  }
  // Called before `node` leaves the tree
  function nodeIteratorsPreRemove(node) {
    for (const w of liveIterators) {
      const it = w.deref();
      if (!it) { liveIterators.delete(w); continue; }
      if (!node.contains(it._ref) || node.contains(it.root)) continue;
      if (it._before) {
        // The first following node not inside `node`
        let n = node;
        while (n && !n.nextSibling && n !== it.root) n = n.parentNode;
        const next = n && n !== it.root ? n.nextSibling : null;
        if (next) { it._ref = next; continue; }
        it._before = false;
      }
      // The node before `node`: its previous sibling's last descendant, or its parent
      let p = node.previousSibling;
      if (p) { while (p.lastChild) p = p.lastChild; it._ref = p; } else it._ref = node.parentNode;
    }
  }
  define(Document.prototype, {
    createTreeWalker(root, whatToShow, filter) { return new TreeWalker(root, whatToShow, filter); },
    createNodeIterator(root, whatToShow, filter) { return new NodeIterator(root, whatToShow, filter); },
  });
  Object.assign(global, { NodeFilter, TreeWalker, NodeIterator });

  // ---- element interfaces ----
  //
  // One prototype per interface, as in browsers: `instanceof` tells
  // elements apart, and reflected attributes (`async`, `value`, `src`...)
  // exist only on the elements that have them, so a custom element class
  // can define its own `value` or `async`. Wrappers get their interface's
  // prototype by tag (__fosSetElementPrototype).
  const elementInterfaces = [];
  function makeInterface(name, parent, tags) {
    // Constructible only as the base of a custom element (`super()`)
    const ctor = ({ [name]: function () { return ceConstruct(new.target); } })[name];
    ctor.prototype = Object.create(parent.prototype);
    Object.defineProperty(ctor.prototype, 'constructor', { value: ctor, writable: true, configurable: true });
    Object.defineProperty(ctor.prototype, Symbol.toStringTag, { value: name, configurable: true });
    Object.setPrototypeOf(ctor, parent);
    Object.defineProperty(global, name, { value: ctor, writable: true, configurable: true });
    elementInterfaces.push(ctor);
    for (const tag of tags ? tags.split(' ') : []) __fosSetElementPrototype(tag, ctor.prototype);
    return ctor;
  }
  for (const [name, tags] of Object.entries({
    HTMLAnchorElement: 'a', HTMLAreaElement: 'area', HTMLBaseElement: 'base', HTMLBodyElement: 'body',
    HTMLBRElement: 'br', HTMLButtonElement: 'button', HTMLCanvasElement: 'canvas', HTMLDataElement: 'data',
    HTMLDataListElement: 'datalist', HTMLDetailsElement: 'details', HTMLDialogElement: 'dialog',
    HTMLDivElement: 'div', HTMLDListElement: 'dl', HTMLEmbedElement: 'embed', HTMLFieldSetElement: 'fieldset',
    HTMLFormElement: 'form', HTMLHeadElement: 'head', HTMLHeadingElement: 'h1 h2 h3 h4 h5 h6',
    HTMLHRElement: 'hr', HTMLHtmlElement: 'html', HTMLIFrameElement: 'iframe', HTMLImageElement: 'img',
    HTMLInputElement: 'input', HTMLLabelElement: 'label', HTMLLegendElement: 'legend', HTMLLIElement: 'li',
    HTMLLinkElement: 'link', HTMLMapElement: 'map', HTMLMenuElement: 'menu', HTMLMetaElement: 'meta',
    HTMLMeterElement: 'meter', HTMLModElement: 'ins del', HTMLObjectElement: 'object', HTMLOListElement: 'ol',
    HTMLOptGroupElement: 'optgroup', HTMLOptionElement: 'option', HTMLOutputElement: 'output',
    HTMLParagraphElement: 'p', HTMLParamElement: 'param', HTMLPictureElement: 'picture',
    HTMLPreElement: 'pre listing xmp', HTMLProgressElement: 'progress', HTMLQuoteElement: 'q blockquote',
    HTMLScriptElement: 'script', HTMLSelectElement: 'select', HTMLSlotElement: 'slot', HTMLSourceElement: 'source',
    HTMLSpanElement: 'span', HTMLStyleElement: 'style', HTMLTableCaptionElement: 'caption',
    HTMLTableCellElement: 'td th', HTMLTableColElement: 'col colgroup', HTMLTableElement: 'table',
    HTMLTableRowElement: 'tr', HTMLTableSectionElement: 'thead tbody tfoot', HTMLTemplateElement: 'template',
    HTMLTextAreaElement: 'textarea', HTMLTimeElement: 'time', HTMLTitleElement: 'title', HTMLTrackElement: 'track',
    HTMLUListElement: 'ul', HTMLUnknownElement: '',
  })) makeInterface(name, HTMLElement, tags);
  makeInterface('HTMLMediaElement', HTMLElement, '');
  makeInterface('HTMLAudioElement', HTMLMediaElement, 'audio');
  makeInterface('HTMLVideoElement', HTMLMediaElement, 'video');
  // SVG elements keep HTMLElement's conveniences (style, dataset, events)
  makeInterface('SVGElement', HTMLElement, 'svg:*');
  makeInterface('SVGGraphicsElement', SVGElement, '');
  makeInterface('SVGSVGElement', SVGGraphicsElement, 'svg:svg');

  // Reflected attributes of particular elements
  const reflectOn = (names, props, descriptor) => {
    for (const name of names.split(' ')) {
      for (const [prop, attr] of props) Object.defineProperty(global[name].prototype, prop, { ...descriptor(attr), configurable: true });
    }
  };
  const stringAttr = (attr) => ({
    get() { return this.getAttribute(attr) ?? ''; },
    set(v) { this.setAttribute(attr, String(v)); },
  });
  const urlAttr = (attr) => ({
    get() { const v = this.getAttribute(attr); return v === null ? '' : __fosResolveURL(v, document.baseURI); },
    set(v) { this.setAttribute(attr, String(v)); },
  });
  const boolAttr = (attr) => ({
    get() { return this.hasAttribute(attr); },
    set(v) { if (v) this.setAttribute(attr, ''); else this.removeAttribute(attr); },
  });
  const intAttr = (fallback) => (attr) => ({
    get() { const v = parseInt(this.getAttribute(attr), 10); return v >= 0 ? v : fallback; },
    set(v) { this.setAttribute(attr, String(Math.max(0, Math.trunc(Number(v)) || 0))); },
  });
  const props = (list) => list.split(' ').map(p => [p, p.toLowerCase()]);
  reflectOn('HTMLButtonElement HTMLFieldSetElement HTMLFormElement HTMLIFrameElement HTMLInputElement HTMLMapElement HTMLMetaElement HTMLObjectElement HTMLOutputElement HTMLSelectElement HTMLSlotElement HTMLTextAreaElement HTMLAnchorElement HTMLImageElement HTMLParamElement', props('name'), stringAttr);
  reflectOn('HTMLAnchorElement HTMLButtonElement HTMLEmbedElement HTMLLinkElement HTMLObjectElement HTMLOListElement HTMLScriptElement HTMLSourceElement HTMLStyleElement HTMLUListElement HTMLLIElement', props('type'), stringAttr);
  reflectOn('HTMLAreaElement HTMLImageElement HTMLInputElement', props('alt'), stringAttr);
  reflectOn('HTMLAnchorElement HTMLAreaElement HTMLLinkElement HTMLFormElement', props('rel'), stringAttr);
  reflectOn('HTMLAnchorElement HTMLAreaElement HTMLBaseElement HTMLFormElement', props('target'), stringAttr);
  reflectOn('HTMLAnchorElement HTMLAreaElement', props('download hreflang ping referrerPolicy'), stringAttr);
  reflectOn('HTMLInputElement HTMLTextAreaElement', props('placeholder dirName'), stringAttr);
  reflectOn('HTMLLabelElement HTMLOutputElement', [['htmlFor', 'for']], stringAttr);
  reflectOn('HTMLInputElement', props('min max step pattern accept'), stringAttr);
  reflectOn('HTMLInputElement HTMLFormElement HTMLSelectElement HTMLTextAreaElement', props('autocomplete'), stringAttr);
  reflectOn('HTMLFormElement', [['method', 'method'], ['enctype', 'enctype'], ['acceptCharset', 'accept-charset']], stringAttr);
  reflectOn('HTMLMetaElement', [['content', 'content'], ['httpEquiv', 'http-equiv']], stringAttr);
  reflectOn('HTMLMetaElement HTMLScriptElement', props('charset'), stringAttr);
  reflectOn('HTMLLinkElement HTMLMetaElement HTMLSourceElement HTMLStyleElement', props('media'), stringAttr);
  reflectOn('HTMLLinkElement HTMLScriptElement HTMLImageElement HTMLMediaElement', [['crossOrigin', 'crossorigin']], stringAttr);
  reflectOn('HTMLLinkElement HTMLScriptElement', props('integrity'), stringAttr);
  reflectOn('HTMLLinkElement', props('as sizes hreflang'), stringAttr);
  reflectOn('HTMLImageElement HTMLSourceElement', props('srcset sizes'), stringAttr);
  reflectOn('HTMLImageElement HTMLIFrameElement', props('loading'), stringAttr);
  reflectOn('HTMLImageElement', props('decoding'), stringAttr);
  reflectOn('HTMLOptionElement HTMLOptGroupElement HTMLTrackElement', props('label'), stringAttr);
  reflectOn('HTMLTrackElement', props('kind srclang'), stringAttr);
  reflectOn('HTMLIFrameElement HTMLEmbedElement HTMLObjectElement', props('width height'), stringAttr);
  reflectOn('HTMLTableCellElement', props('headers abbr scope'), stringAttr);
  reflectOn('HTMLTimeElement HTMLModElement', [['dateTime', 'datetime']], stringAttr);
  reflectOn('HTMLAnchorElement HTMLAreaElement HTMLBaseElement HTMLLinkElement', props('href'), urlAttr);
  reflectOn('HTMLMediaElement HTMLEmbedElement HTMLIFrameElement HTMLImageElement HTMLInputElement HTMLScriptElement HTMLSourceElement HTMLTrackElement', props('src'), urlAttr);
  reflectOn('HTMLFormElement', props('action'), urlAttr);
  reflectOn('HTMLButtonElement HTMLInputElement', [['formAction', 'formaction']], urlAttr);
  reflectOn('HTMLVideoElement', props('poster'), urlAttr);
  reflectOn('HTMLQuoteElement HTMLModElement', props('cite'), urlAttr);
  reflectOn('HTMLObjectElement', props('data'), urlAttr);
  reflectOn('HTMLButtonElement HTMLFieldSetElement HTMLInputElement HTMLLinkElement HTMLOptGroupElement HTMLOptionElement HTMLSelectElement HTMLTextAreaElement', props('disabled'), boolAttr);
  reflectOn('HTMLInputElement', [['checked', 'checked'], ['defaultChecked', 'checked'], ['indeterminate', 'indeterminate']], boolAttr);
  reflectOn('HTMLOptionElement', [['selected', 'selected'], ['defaultSelected', 'selected']], boolAttr);
  reflectOn('HTMLInputElement HTMLTextAreaElement', props('readOnly'), boolAttr);
  reflectOn('HTMLInputElement HTMLSelectElement HTMLTextAreaElement', props('required'), boolAttr);
  reflectOn('HTMLInputElement HTMLSelectElement', props('multiple'), boolAttr);
  reflectOn('HTMLScriptElement', props('async defer noModule'), boolAttr);
  reflectOn('HTMLFormElement', props('noValidate'), boolAttr);
  reflectOn('HTMLButtonElement HTMLInputElement', props('formNoValidate'), boolAttr);
  reflectOn('HTMLDetailsElement HTMLDialogElement', props('open'), boolAttr);
  reflectOn('HTMLMediaElement', [['controls', 'controls'], ['autoplay', 'autoplay'], ['loop', 'loop'], ['muted', 'muted'], ['defaultMuted', 'muted']], boolAttr);
  reflectOn('HTMLVideoElement', props('playsInline'), boolAttr);
  reflectOn('HTMLImageElement', props('isMap'), boolAttr);
  reflectOn('HTMLOListElement', props('reversed'), boolAttr);
  reflectOn('HTMLImageElement HTMLVideoElement HTMLInputElement', props('width height'), intAttr(0));
  reflectOn('HTMLCanvasElement', props('width'), intAttr(300));
  reflectOn('HTMLCanvasElement', props('height'), intAttr(150));
  reflectOn('HTMLTextAreaElement', props('rows'), intAttr(2));
  reflectOn('HTMLTextAreaElement', props('cols'), intAttr(20));
  reflectOn('HTMLInputElement', props('size'), intAttr(20));
  reflectOn('HTMLTableCellElement', props('colSpan rowSpan'), intAttr(1));
  reflectOn('HTMLInputElement HTMLTextAreaElement', [['maxLength', 'maxlength'], ['minLength', 'minlength']], intAttr(-1));
  // Form control values live in the `value` attribute (no separate dirty value)
  const valueAttr = (fallback) => ({
    get() { return this.getAttribute('value') ?? fallback(this); },
    set(v) { this.setAttribute('value', v == null ? '' : String(v)); },
    configurable: true,
  });
  for (const name of ['HTMLButtonElement', 'HTMLDataElement', 'HTMLParamElement']) {
    Object.defineProperty(global[name].prototype, 'value', valueAttr(() => ''));
  }
  Object.defineProperty(HTMLInputElement.prototype, 'value', valueAttr((el) => el.type === 'checkbox' || el.type === 'radio' ? 'on' : ''));
  Object.defineProperty(HTMLInputElement.prototype, 'defaultValue', valueAttr(() => ''));
  Object.defineProperty(HTMLOptionElement.prototype, 'value', valueAttr((el) => el.textContent.trim()));
  Object.defineProperty(HTMLOptionElement.prototype, 'text', { get() { return this.textContent.trim(); }, set(v) { this.textContent = v; }, configurable: true });
  for (const name of ['HTMLTextAreaElement', 'HTMLOutputElement']) {
    for (const prop of ['value', 'defaultValue']) {
      Object.defineProperty(global[name].prototype, prop, { get() { return this.textContent; }, set(v) { this.textContent = String(v); }, configurable: true });
    }
  }
  Object.defineProperty(HTMLSelectElement.prototype, 'value', {
    get() {
      const opt = this.querySelector('option[selected]') || this.querySelector('option');
      return opt ? opt.value : '';
    },
    set(v) { for (const o of this.querySelectorAll('option')) o.selected = o.value === String(v); },
    configurable: true,
  });
  define(HTMLSelectElement.prototype, {
    get options() { return this.querySelectorAll('option'); },
    get selectedIndex() { return Array.prototype.findIndex.call(this.querySelectorAll('option'), o => o.selected); },
    set selectedIndex(i) { Array.prototype.forEach.call(this.querySelectorAll('option'), (o, j) => { o.selected = j === i; }); },
    get length() { return this.querySelectorAll('option').length; },
  });
  for (const name of ['HTMLLIElement', 'HTMLMeterElement', 'HTMLProgressElement']) {
    Object.defineProperty(global[name].prototype, 'value', {
      get() { const v = parseFloat(this.getAttribute('value')); return isNaN(v) ? 0 : v; },
      set(v) { this.setAttribute('value', String(v)); },
      configurable: true,
    });
  }
  define(HTMLScriptElement.prototype, {
    get text() { return this.textContent; },
    set text(v) { this.textContent = v; },
  });
  // Interfaces scripts test for or patch (polyfills walk them), and
  // character data the page parser does not produce. Shadow roots are
  // still a stand-in (attachShadow returns the host), so none is made.
  for (const [name, parent] of [['CDATASection', Text], ['ProcessingInstruction', CharacterData], ['ShadowRoot', DocumentFragment],
    ['Attr', Node], ['DocumentType', Node]]) {
    const ctor = ({ [name]: function () { throw new TypeError('Illegal constructor'); } })[name];
    ctor.prototype = Object.create(parent.prototype, { constructor: { value: ctor, writable: true, configurable: true } });
    Object.setPrototypeOf(ctor, parent);
    global[name] = ctor;
  }
  // `window instanceof Window`, with Window.prototype on the global's chain
  {
    const WindowCtor = function Window() { throw new TypeError('Illegal constructor'); };
    let proto = Object.getPrototypeOf(global);
    if (proto === Object.prototype || proto === null) {
      proto = Object.create(EventTarget.prototype);
      Object.setPrototypeOf(global, proto);
    }
    WindowCtor.prototype = proto;
    Object.defineProperty(proto, 'constructor', { value: WindowCtor, writable: true, configurable: true });
    Object.defineProperty(proto, Symbol.toStringTag, { value: 'Window', configurable: true });
    global.Window = WindowCtor;
  }
  // Legacy factory constructors
  function Image(width, height) {
    const img = document.createElement('img');
    if (width !== undefined) img.setAttribute('width', String(width));
    if (height !== undefined) img.setAttribute('height', String(height));
    return img;
  }
  function Audio(src) {
    const audio = document.createElement('audio');
    audio.setAttribute('preload', 'auto');
    if (src !== undefined) audio.setAttribute('src', String(src));
    return audio;
  }
  function Option(text = '', value, defaultSelected = false, selected = false) {
    const option = document.createElement('option');
    option.textContent = String(text);
    if (value !== undefined) option.setAttribute('value', String(value));
    if (defaultSelected) option.setAttribute('selected', '');
    if (selected) option.selected = true;
    return option;
  }
  for (const f of [Image, Audio, Option]) f.prototype = HTMLElement.prototype;
  Object.assign(global, { Image, Audio, Option });

  // ---- custom elements ----
  //
  // A registry of element definitions. Elements are upgraded (their
  // wrapper gets the class's prototype and the constructor runs on it)
  // when a definition arrives for elements already in the document, when
  // created with createElement or `new`, and when inserted. Lifecycle
  // callbacks run synchronously after the DOM operation that causes them.
  // Nothing here costs anything until a page defines an element.
  const NativeHTMLElement = global.HTMLElement;
  const ceDefs = new Map();      // name -> definition
  const ceByCtor = new Map();    // constructor -> definition
  const ceWaiting = new Map();   // name -> { promise, resolve }
  const ceState = new WeakMap(); // element -> 'custom' | 'failed'
  let ceDefining = false;
  const ceReserved = new Set(['annotation-xml', 'color-profile', 'font-face', 'font-face-src', 'font-face-uri',
    'font-face-format', 'font-face-name', 'missing-glyph']);
  const ceValidName = (n) => /^[a-z][-.0-9_a-z·À-￿]*$/.test(n) && n.includes('-') && !ceReserved.has(n);
  const rawCreateElement = Document.prototype.createElement;

  // `super()` in a custom element class: the element being upgraded, or a
  // new one for `new MyElement()`
  const HTMLElementCtor = function HTMLElement() { return ceConstruct(new.target); };
  // `super()` reaching an element interface's constructor
  function ceConstruct(newTarget) {
    const def = newTarget && ceByCtor.get(newTarget);
    if (!def) throw new TypeError('Illegal constructor');
    let el = def.stack.pop();
    if (!el) {
      el = rawCreateElement.call(document, def.extends || def.name);
      if (def.extends) el.setAttribute('is', def.name);
      ceState.set(el, 'custom');
    }
    Object.setPrototypeOf(el, newTarget.prototype);
    return el;
  }
  HTMLElementCtor.prototype = NativeHTMLElement.prototype;
  Object.defineProperty(NativeHTMLElement.prototype, 'constructor', { value: HTMLElementCtor, writable: true, configurable: true });
  global.HTMLElement = HTMLElementCtor;
  for (const c of elementInterfaces) {
    if (Object.getPrototypeOf(c) === NativeHTMLElement) Object.setPrototypeOf(c, HTMLElementCtor);
  }

  function ceDefinitionOf(el) {
    if (el.nodeType !== 1) return undefined;
    const def = ceDefs.get(el.localName);
    if (def && !def.extends) return def;
    const is = el.getAttribute('is');
    const custom = is && ceDefs.get(is);
    return custom && custom.extends === el.localName ? custom : undefined;
  }

  function ceCallback(el, name, args) {
    if (ceState.get(el) !== 'custom') return;
    const def = ceDefinitionOf(el);
    const f = def && def.callbacks[name];
    if (!f) return;
    try { f.apply(el, args); } catch (e) { reportError(e); }
  }

  function ceUpgrade(el) {
    if (ceState.has(el)) return;
    const def = ceDefinitionOf(el);
    if (!def) return;
    ceState.set(el, 'failed');
    def.stack.push(el);
    try {
      const made = new def.ctor();
      if (made !== el) throw new DOMException('The custom element constructor did not produce the element being upgraded.', 'InvalidStateError');
    } catch (e) {
      reportError(e);
      def.stack.length = 0;
      return;
    }
    ceState.set(el, 'custom');
    for (const attr of def.observed) {
      if (el.hasAttribute(attr)) ceCallback(el, 'attributeChangedCallback', [attr, null, el.getAttribute(attr)]);
    }
    if (el.isConnected) ceCallback(el, 'connectedCallback', []);
  }

  // Elements of `root`'s subtree (root included), in tree order
  function ceSubtree(root) {
    if (!root || (root.nodeType !== 1 && root.nodeType !== 11 && root.nodeType !== 9)) return [];
    const all = Array.from(root.querySelectorAll('*'));
    if (root.nodeType === 1) all.unshift(root);
    return all;
  }

  // `nodes` were inserted: upgrade what was waiting, connect what is custom
  function ceInserted(nodes) {
    if (!ceDefs.size) return;
    for (const node of nodes) {
      if (!node || !node.isConnected) continue;
      for (const el of ceSubtree(node)) {
        if (ceState.get(el) === 'custom') ceCallback(el, 'connectedCallback', []);
        else ceUpgrade(el);
      }
    }
  }

  // The custom elements under `nodes` that are connected (before a removal)
  function ceConnectedIn(nodes) {
    if (!ceDefs.size) return [];
    const out = [];
    for (const node of nodes) {
      if (!node || !node.isConnected) continue;
      for (const el of ceSubtree(node)) if (ceState.get(el) === 'custom') out.push(el);
    }
    return out;
  }
  function ceRemoved(list) {
    for (const el of list) if (!el.isConnected) ceCallback(el, 'disconnectedCallback', []);
  }

  // Nodes an insertion call adds (fragments add their children)
  const ceArgNodes = (args) => {
    const out = [];
    for (const a of args) {
      if (a && typeof a === 'object' && a.nodeType) {
        if (a.nodeType === 11) out.push(...a.childNodes); else out.push(a);
      }
    }
    return out;
  };
  const NodeP = Node.prototype, ElementP = NativeHTMLElement.prototype;
  const ceWrap = (proto, name, plan) => {
    const original = proto[name];
    if (typeof original !== 'function') return;
    define(proto, { [name](...args) {
      if (!ceDefs.size) return original.apply(this, args);
      const { added, removed } = plan.call(this, args);
      const gone = ceConnectedIn(removed);
      const r = original.apply(this, args);
      ceRemoved(gone);
      ceInserted(added);
      return r;
    } });
  };
  ceWrap(NodeP, 'appendChild', (a) => ({ added: ceArgNodes(a.slice(0, 1)), removed: [] }));
  ceWrap(NodeP, 'insertBefore', (a) => ({ added: ceArgNodes(a.slice(0, 1)), removed: [] }));
  ceWrap(NodeP, 'replaceChild', (a) => ({ added: ceArgNodes(a.slice(0, 1)), removed: [a[1]] }));
  ceWrap(NodeP, 'removeChild', (a) => ({ added: [], removed: [a[0]] }));
  for (const name of ['append', 'prepend', 'before', 'after']) {
    ceWrap(NodeP, name, (a) => ({ added: ceArgNodes(a), removed: [] }));
    ceWrap(ElementP, name, (a) => ({ added: ceArgNodes(a), removed: [] }));
  }
  for (const proto of [NodeP, ElementP]) {
    ceWrap(proto, 'replaceWith', function (a) { return { added: ceArgNodes(a), removed: [this] }; });
    ceWrap(proto, 'remove', function () { return { added: [], removed: [this] }; });
  }
  ceWrap(ElementP, 'insertAdjacentHTML', function (a) {
    const where = String(a[0]).toLowerCase();
    return { added: [where === 'beforebegin' || where === 'afterend' ? this.parentNode : this], removed: [] };
  });
  // Markup setters replace the children
  for (const [proto, name] of [[ElementP, 'innerHTML'], [NodeP, 'textContent'], [ElementP, 'outerHTML']]) {
    let owner = proto, d;
    while (owner && !(d = Object.getOwnPropertyDescriptor(owner, name))) owner = Object.getPrototypeOf(owner);
    if (!d || !d.set) continue;
    Object.defineProperty(proto, name, {
      configurable: true,
      get: d.get,
      set(v) {
        if (!ceDefs.size) return d.set.call(this, v);
        const outer = name === 'outerHTML';
        const parent = this.parentNode;
        const gone = ceConnectedIn(outer ? [this] : Array.from(this.childNodes));
        d.set.call(this, v);
        ceRemoved(gone);
        ceInserted([outer ? parent : this]);
      },
    });
  }
  // Removals move live NodeIterators off the removed subtree
  const iterWrap = (proto, name, removed) => {
    const original = proto[name];
    if (typeof original !== 'function') return;
    define(proto, { [name](...args) {
      if (liveIterators.size) for (const n of removed.call(this, args)) if (n && n.parentNode) nodeIteratorsPreRemove(n);
      return original.apply(this, args);
    } });
  };
  iterWrap(NodeP, 'removeChild', (a) => [a[0]]);
  iterWrap(NodeP, 'replaceChild', (a) => [a[1]]);
  for (const proto of [NodeP, ElementP, CharacterData.prototype]) {
    iterWrap(proto, 'remove', function () { return [this]; });
    iterWrap(proto, 'replaceWith', function () { return [this]; });
  }
  for (const [proto, name] of [[ElementP, 'innerHTML'], [NodeP, 'textContent']]) {
    let owner = proto, d;
    while (owner && !(d = Object.getOwnPropertyDescriptor(owner, name))) owner = Object.getPrototypeOf(owner);
    if (!d || !d.set) continue;
    Object.defineProperty(proto, name, {
      configurable: true,
      get: d.get,
      set(v) {
        if (liveIterators.size) for (const n of Array.from(this.childNodes)) nodeIteratorsPreRemove(n);
        d.set.call(this, v);
      },
    });
  }
  // Observed attributes
  for (const name of ['setAttribute', 'removeAttribute', 'toggleAttribute']) {
    const original = ElementP[name] || NodeP[name];
    if (typeof original !== 'function') continue;
    define(ElementP, { [name](...args) {
      if (ceState.get(this) !== 'custom') return original.apply(this, args);
      const attr = String(args[0]).toLowerCase();
      const old = this.getAttribute(attr);
      const r = original.apply(this, args);
      const def = ceDefinitionOf(this);
      if (def && def.observed.has(attr)) {
        const now = this.getAttribute(attr);
        if (!(old === null && now === null)) ceCallback(this, 'attributeChangedCallback', [attr, old, now]);
      }
      return r;
    } });
  }
  define(Document.prototype, { createElement(name, options) {
    const el = rawCreateElement.call(this, name);
    if (options && typeof options === 'object' && options.is) el.setAttribute('is', String(options.is));
    if (ceDefs.size) ceUpgrade(el);
    return el;
  } });

  class CustomElementRegistry {
    define(name, ctor, options) {
      name = String(name);
      if (typeof ctor !== 'function') throw new TypeError("Failed to execute 'define' on 'CustomElementRegistry': The provided value is not a constructor.");
      if (!ceValidName(name)) throw new DOMException(`Failed to execute 'define' on 'CustomElementRegistry': "${name}" is not a valid custom element name`, 'SyntaxError');
      if (ceDefs.has(name)) throw new DOMException(`Failed to execute 'define' on 'CustomElementRegistry': the name "${name}" has already been used with this registry`, 'NotSupportedError');
      if (ceByCtor.has(ctor)) throw new DOMException("Failed to execute 'define' on 'CustomElementRegistry': this constructor has already been used with this registry", 'NotSupportedError');
      if (ceDefining) throw new DOMException('Custom element definitions cannot be nested', 'NotSupportedError');
      const ext = options && options.extends ? String(options.extends).toLowerCase() : null;
      ceDefining = true;
      let callbacks, observed;
      try {
        const proto = ctor.prototype;
        if (proto === null || typeof proto !== 'object') throw new TypeError('The constructor\'s prototype is not an object');
        callbacks = {};
        for (const k of ['connectedCallback', 'disconnectedCallback', 'adoptedCallback', 'attributeChangedCallback']) {
          const f = proto[k];
          if (f !== undefined && typeof f !== 'function') throw new TypeError(`${k} is not a function`);
          callbacks[k] = f;
        }
        observed = new Set(callbacks.attributeChangedCallback && ctor.observedAttributes ? Array.from(ctor.observedAttributes, a => String(a)) : []);
      } finally {
        ceDefining = false;
      }
      const def = { name, ctor, extends: ext, callbacks, observed, stack: [] };
      ceDefs.set(name, def);
      ceByCtor.set(ctor, def);
      const selector = ext ? `${ext}[is="${name}"]` : CSS.escape(name);
      for (const el of document.querySelectorAll(selector)) ceUpgrade(el);
      const waiting = ceWaiting.get(name);
      if (waiting) { ceWaiting.delete(name); waiting.resolve(ctor); }
    }
    get(name) { return ceDefs.get(String(name))?.ctor; }
    getName(ctor) { return ceByCtor.get(ctor)?.name ?? null; }
    whenDefined(name) {
      name = String(name);
      if (!ceValidName(name)) return Promise.reject(new DOMException(`"${name}" is not a valid custom element name`, 'SyntaxError'));
      if (ceDefs.has(name)) return Promise.resolve(ceDefs.get(name).ctor);
      let w = ceWaiting.get(name);
      if (!w) {
        let resolve;
        const promise = new Promise(r => { resolve = r; });
        ceWaiting.set(name, w = { promise, resolve });
      }
      return w.promise;
    }
    upgrade(root) { for (const el of ceSubtree(root)) ceUpgrade(el); }
  }
  // ElementInternals, as far as scripts commonly use it
  class ElementInternals {
    constructor(el) { Object.defineProperty(this, '_el', { value: el }); this.states = new Set(); this.validity = { valid: true }; this.validationMessage = ''; this.willValidate = false; }
    get form() { return this._el.closest('form'); }
    get labels() { return []; }
    get shadowRoot() { return null; }
    setFormValue() {}
    setValidity(flags = {}, message = '') { this.validity = { ...flags, valid: !Object.values(flags).some(Boolean) }; this.validationMessage = String(message); }
    checkValidity() { return this.validity.valid; }
    reportValidity() { return this.validity.valid; }
  }
  define(ElementP, {
    attachInternals() {
      if (!ceDefinitionOf(this)) throw new DOMException("Failed to execute 'attachInternals' on 'HTMLElement': Unable to attach ElementInternals to non-custom elements.", 'NotSupportedError');
      return new ElementInternals(this);
    },
  });
  Object.assign(global, { customElements: new CustomElementRegistry(), CustomElementRegistry, ElementInternals });

  // <template>: its parsed children live in a fragment outside the page
  Object.defineProperty(ElementP, 'content', {
    configurable: true,
    get() { return this.localName === 'template' ? __fosTemplateContent(this) : undefined; },
  });

  // ---- crypto (random values only; no SubtleCrypto yet) ----
  const integerArrays = ['Int8Array', 'Uint8Array', 'Uint8ClampedArray', 'Int16Array', 'Uint16Array', 'Int32Array', 'Uint32Array', 'BigInt64Array', 'BigUint64Array'];
  class Crypto {
    getRandomValues(array) {
      const kind = array && Object.prototype.toString.call(array).slice(8, -1);
      if (!integerArrays.includes(kind)) {
        throw new DOMException("Failed to execute 'getRandomValues' on 'Crypto': The provided ArrayBufferView is not an integer array type.", 'TypeMismatchError');
      }
      if (array.byteLength > 65536) {
        throw new DOMException(`Failed to execute 'getRandomValues' on 'Crypto': The ArrayBufferView's byte length (${array.byteLength}) exceeds the number of bytes of entropy available via this API (65536).`, 'QuotaExceededError');
      }
      new Uint8Array(array.buffer, array.byteOffset, array.byteLength).set(new Uint8Array(__fosRandomBytes(array.byteLength)));
      return array;
    }
    randomUUID() {
      const b = new Uint8Array(__fosRandomBytes(16));
      b[6] = (b[6] & 0x0f) | 0x40;
      b[8] = (b[8] & 0x3f) | 0x80;
      const h = Array.from(b, x => x.toString(16).padStart(2, '0')).join('');
      return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
    }
  }
  Object.assign(global, { crypto: new Crypto(), Crypto });

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
    get cookie() { return __fosCookie(); },
    set cookie(v) { __fosSetCookie(String(v)); },
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

  // Session history of the page: pushState/replaceState change the URL
  // (the browser shows it) without loading anything
  class History {
    constructor() {
      Object.defineProperties(this, {
        _entries: { value: [{ state: null, url: document.URL }] },
        _index: { value: 0, writable: true },
      });
      this.scrollRestoration = 'auto';
    }
    get length() { return this._entries.length; }
    get state() { return this._entries[this._index].state; }
    _target(url) {
      if (url === undefined || url === null) return global.location.href;
      const u = new URL(String(url), global.location.href);
      if (u.origin !== global.location.origin) {
        throw new DOMException(`A history state object with URL '${u.href}' cannot be created in a document with origin '${global.location.origin}'.`, 'SecurityError');
      }
      return u.href;
    }
    pushState(state, _title, url) {
      const target = this._target(url);
      this._entries.splice(this._index + 1);
      this._entries.push({ state: structuredCloneState(state), url: target });
      this._index++;
      setDocumentURL(target);
    }
    replaceState(state, _title, url) {
      const target = this._target(url);
      this._entries[this._index] = { state: structuredCloneState(state), url: target };
      setDocumentURL(target);
    }
    back() { this.go(-1); }
    forward() { this.go(1); }
    go(delta = 0) {
      delta = Math.trunc(+delta) || 0;
      if (delta === 0) { global.location.reload(); return; }
      const index = this._index + delta;
      if (index < 0 || index >= this._entries.length) return;
      const oldURL = global.location.href;
      this._index = index;
      const entry = this._entries[index];
      setDocumentURL(entry.url);
      setTimeout(() => {
        global.dispatchEvent(new PopStateEvent('popstate', { state: entry.state }));
        if (oldURL.split('#')[0] === entry.url.split('#')[0] && oldURL !== entry.url) {
          global.dispatchEvent(new HashChangeEvent('hashchange', { oldURL, newURL: entry.url }));
        }
      }, 0);
    }
  }
  const structuredCloneState = (v) => (v === undefined ? null : v);
  // The URL changes, the page stays (the browser picks it up after the task)
  function setDocumentURL(url) {
    if (global.location) global.location._url = new URL(url);
    __fosSetURL(url);
  }

  class Storage {
    constructor() { Object.defineProperty(this, '_m', { value: new Map() }); }
    get length() { return this._m.size; }
    key(i) { return [...this._m.keys()][i] ?? null; }
    getItem(k) { return this._m.has(String(k)) ? this._m.get(String(k)) : null; }
    setItem(k, v) { this._m.set(String(k), String(v)); }
    removeItem(k) { this._m.delete(String(k)); }
    clear() { this._m.clear(); }
  }

  // ---- networking and binary data ----

  class DOMException extends Error {
    constructor(message = '', name = 'Error') { super(message); this.name = String(name); }
    get code() {
      return ({ IndexSizeError: 1, HierarchyRequestError: 3, NotFoundError: 8, NotSupportedError: 9,
        InvalidStateError: 11, SyntaxError: 12, InvalidAccessError: 15, SecurityError: 18,
        NetworkError: 19, AbortError: 20, TimeoutError: 23, DataCloneError: 25 })[this.name] || 0;
    }
  }

  // EventTarget can be constructed and extended by scripts
  const EventTargetCtor = function EventTarget() {
    if (!new.target) throw new TypeError("Failed to construct 'EventTarget': Please use the 'new' operator");
  };
  EventTargetCtor.prototype = EventTargetProto;
  Object.defineProperty(EventTargetProto, 'constructor', { value: EventTargetCtor, writable: true, configurable: true });

  class AbortSignal extends EventTargetCtor {
    constructor() { super(); this.aborted = false; this.reason = undefined; }
    throwIfAborted() { if (this.aborted) throw this.reason; }
    _abort(reason) {
      if (this.aborted) return;
      this.aborted = true;
      this.reason = reason === undefined ? new DOMException('signal is aborted without reason', 'AbortError') : reason;
      this.dispatchEvent(new Event('abort'));
    }
    static abort(reason) { const s = new AbortSignal(); s._abort(reason); return s; }
    static timeout(ms) {
      const s = new AbortSignal();
      setTimeout(() => s._abort(new DOMException('signal timed out', 'TimeoutError')), ms);
      return s;
    }
    static any(signals) {
      const s = new AbortSignal();
      for (const t of signals) {
        if (t.aborted) { s._abort(t.reason); break; }
        t.addEventListener('abort', () => s._abort(t.reason));
      }
      return s;
    }
  }
  class AbortController {
    constructor() { this.signal = new AbortSignal(); }
    abort(reason) { this.signal._abort(reason); }
  }

  class ProgressEvent extends Event {
    constructor(type, init = {}) {
      super(type, init);
      this.lengthComputable = !!init.lengthComputable;
      this.loaded = init.loaded || 0;
      this.total = init.total || 0;
    }
  }

  const labelName = l => {
    l = String(l).trim().toLowerCase();
    if (/16be|unicodefffe/.test(l)) return 'utf-16be';
    if (/utf-16|ucs-2|^unicode$|csunicode|iso-10646/.test(l)) return 'utf-16le';
    if (/8859-1|1252|latin1|ascii|^l1$|819|iso-ir-100|x3\.4/.test(l)) return 'windows-1252';
    return 'utf-8';
  };
  class TextEncoder {
    get encoding() { return 'utf-8'; }
    encode(s = '') { return new Uint8Array(__fosEncode(String(s))); }
    encodeInto(s, dest) {
      const b = this.encode(s);
      const n = Math.min(b.length, dest.length);
      dest.set(n === b.length ? b : b.subarray(0, n));
      return { read: n === b.length ? String(s).length : __fosDecode(b.subarray(0, n)).length, written: n };
    }
  }
  class TextDecoder {
    constructor(label = 'utf-8', options = {}) {
      __fosDecode(new ArrayBuffer(0), String(label)); // throws RangeError for unknown labels
      this._label = String(label);
      this.fatal = !!options.fatal;
      this.ignoreBOM = !!options.ignoreBOM;
    }
    get encoding() { return labelName(this._label); }
    // `{stream: true}` keeps an incomplete UTF-8 sequence for the next call
    decode(input, options) {
      const stream = !!(options && options.stream);
      let bytes = input === undefined ? new Uint8Array(0) : new Uint8Array(toArrayBuffer(input));
      if (this._pending && this._pending.length) {
        const joined = new Uint8Array(this._pending.length + bytes.length);
        joined.set(this._pending);
        joined.set(bytes, this._pending.length);
        bytes = joined;
      }
      this._pending = null;
      if (stream && this.encoding === 'utf-8') {
        // Back up to the start of a trailing sequence that is not complete
        let cut = bytes.length;
        for (let i = bytes.length - 1, n = 0; i >= 0 && n < 4; i--, n++) {
          const b = bytes[i];
          if ((b & 0xC0) === 0x80) continue;
          const need = b >= 0xF0 ? 4 : b >= 0xE0 ? 3 : b >= 0xC0 ? 2 : 1;
          if (bytes.length - i < need) cut = i;
          break;
        }
        this._pending = bytes.slice(cut);
        bytes = bytes.slice(0, cut);
      }
      return bytes.length ? __fosDecode(bytes, this._label) : '';
    }
  }

  // Bytes as a fresh ArrayBuffer
  const toArrayBuffer = b =>
    b == null ? new ArrayBuffer(0)
      : typeof b === 'string' ? __fosEncode(b)
      : b instanceof ArrayBuffer ? b.slice(0)
      : b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength);

  class Blob {
    constructor(parts = [], options = {}) {
      const chunks = [];
      let size = 0;
      for (const p of parts) {
        const c = p instanceof Blob ? new Uint8Array(p._buf)
          : p instanceof ArrayBuffer ? new Uint8Array(p)
          : ArrayBuffer.isView(p) ? new Uint8Array(p.buffer, p.byteOffset, p.byteLength)
          : new Uint8Array(__fosEncode(String(p)));
        chunks.push(c);
        size += c.length;
      }
      const all = new Uint8Array(size);
      let at = 0;
      for (const c of chunks) { all.set(c, at); at += c.length; }
      this._buf = all.buffer;
      this.type = options.type ? String(options.type).toLowerCase() : '';
    }
    get size() { return this._buf.byteLength; }
    slice(start = 0, end = this.size, type = '') {
      const clamp = v => v < 0 ? Math.max(this.size + v, 0) : Math.min(v, this.size);
      const b = new Blob([], { type });
      b._buf = this._buf.slice(clamp(start), clamp(end));
      return b;
    }
    text() { return Promise.resolve(__fosDecode(this._buf, 'utf-8')); }
    arrayBuffer() { return Promise.resolve(this._buf.slice(0)); }
    bytes() { return Promise.resolve(new Uint8Array(this._buf.slice(0))); }
    stream() { const buf = this._buf; return bytesStream(() => new Uint8Array(buf.slice(0))); }
    get [Symbol.toStringTag]() { return 'Blob'; }
  }
  class File extends Blob {
    constructor(parts, name, options = {}) {
      super(parts, options);
      this.name = String(name);
      this.lastModified = options.lastModified ?? Date.now();
    }
    get [Symbol.toStringTag]() { return 'File'; }
  }

  // Only what reading a whole body at once needs: one chunk, then done
  // ---- streams (WHATWG Streams: queues, backpressure, piping) ----

  class ReadableStreamDefaultController {
    constructor(stream) { Object.defineProperty(this, '_s', { value: stream }); }
    get desiredSize() { const s = this._s; return s._state === 'errored' ? null : s._state === 'closed' ? 0 : s._hwm - s._queueSize; }
    enqueue(chunk) { this._s._enqueue(chunk); }
    close() { this._s._requestClose(); }
    error(e) { this._s._error(e); }
  }

  class ReadableStreamDefaultReader {
    constructor(stream) {
      if (!(stream instanceof ReadableStream)) throw new TypeError('ReadableStreamDefaultReader needs a ReadableStream');
      if (stream._reader) throw new TypeError('ReadableStream is locked');
      Object.defineProperty(this, '_s', { value: stream, writable: true });
      stream._reader = this;
      this._closed = deferred();
      if (stream._state === 'closed') this._closed.resolve();
      if (stream._state === 'errored') { this._closed.reject(stream._storedError); this._closed.promise.catch(() => {}); }
    }
    get closed() { return this._closed.promise; }
    read() {
      if (!this._s) return Promise.reject(new TypeError('This reader has been released'));
      return this._s._read();
    }
    releaseLock() {
      const s = this._s;
      if (!s) return;
      for (const r of s._readRequests.splice(0)) r.reject(new TypeError('Reader was released'));
      if (s._state === 'readable') { this._closed.reject(new TypeError('Reader was released')); this._closed.promise.catch(() => {}); }
      s._reader = null;
      this._s = null;
    }
    cancel(reason) { return this._s ? this._s._cancel(reason) : Promise.reject(new TypeError('This reader has been released')); }
  }

  function deferred() {
    let resolve, reject;
    const promise = new Promise((a, b) => { resolve = a; reject = b; });
    return { promise, resolve, reject };
  }

  class ReadableStream {
    constructor(source = {}, strategy = {}) {
      source = source ?? {};
      Object.defineProperties(this, {
        _source: { value: source }, _queue: { value: [], writable: true }, _state: { value: 'readable', writable: true },
        _reader: { value: null, writable: true }, _readRequests: { value: [] }, _storedError: { value: undefined, writable: true },
        _closeRequested: { value: false, writable: true }, _pulling: { value: false, writable: true }, _pullAgain: { value: false, writable: true },
        _started: { value: false, writable: true }, _queueSize: { value: 0, writable: true },
        _hwm: { value: strategy.highWaterMark ?? (source.type === 'bytes' ? 0 : 1) }, _size: { value: strategy.size },
      });
      const controller = new ReadableStreamDefaultController(this);
      Object.defineProperty(this, '_controller', { value: controller });
      let started;
      try { started = source.start ? source.start.call(source, controller) : undefined; } catch (e) { this._error(e); started = undefined; }
      Promise.resolve(started).then(() => { this._started = true; this._pullIfNeeded(); }, (e) => this._error(e));
    }
    get locked() { return !!this._reader; }
    _enqueue(chunk) {
      if (this._closeRequested || this._state !== 'readable') throw new TypeError('Cannot enqueue a chunk into a closed stream');
      const reqs = this._readRequests;
      if (reqs.length) reqs.shift().resolve({ value: chunk, done: false });
      else {
        const size = this._size ? Number(this._size(chunk)) : 1;
        this._queue.push({ chunk, size });
        this._queueSize += size;
      }
      this._pullIfNeeded();
    }
    _requestClose() {
      if (this._closeRequested || this._state !== 'readable') throw new TypeError('The stream is not in a state that permits close');
      this._closeRequested = true;
      if (!this._queue.length) this._close();
    }
    _close() {
      if (this._state !== 'readable') return;
      this._state = 'closed';
      for (const r of this._readRequests.splice(0)) r.resolve({ value: undefined, done: true });
      if (this._reader) this._reader._closed.resolve();
    }
    _error(e) {
      if (this._state !== 'readable') return;
      this._state = 'errored';
      this._storedError = e;
      this._queue = [];
      this._queueSize = 0;
      for (const r of this._readRequests.splice(0)) r.reject(e);
      if (this._reader) { this._reader._closed.reject(e); this._reader._closed.promise.catch(() => {}); }
    }
    _pullIfNeeded() {
      if (!this._started || this._state !== 'readable' || this._closeRequested || !this._source.pull) return;
      if (!this._readRequests.length && this._hwm - this._queueSize <= 0) return;
      if (this._pulling) { this._pullAgain = true; return; }
      this._pulling = true;
      Promise.resolve()
        .then(() => this._source.pull.call(this._source, this._controller))
        .then(() => {
          this._pulling = false;
          if (this._pullAgain) { this._pullAgain = false; this._pullIfNeeded(); }
        }, (e) => this._error(e));
    }
    _read() {
      if (this._queue.length) {
        const { chunk, size } = this._queue.shift();
        this._queueSize -= size;
        if (this._closeRequested && !this._queue.length) this._close();
        else this._pullIfNeeded();
        return Promise.resolve({ value: chunk, done: false });
      }
      if (this._state === 'closed') return Promise.resolve({ value: undefined, done: true });
      if (this._state === 'errored') return Promise.reject(this._storedError);
      const d = deferred();
      this._readRequests.push(d);
      this._pullIfNeeded();
      return d.promise;
    }
    _cancel(reason) {
      if (this._state === 'closed') return Promise.resolve();
      if (this._state === 'errored') return Promise.reject(this._storedError);
      this._queue = [];
      this._queueSize = 0;
      this._close();
      const cancel = this._source.cancel;
      return Promise.resolve().then(() => cancel && cancel.call(this._source, reason)).then(() => undefined);
    }
    getReader(options) {
      if (options && options.mode !== undefined && options.mode !== 'byob') throw new TypeError('Invalid reader mode');
      return new ReadableStreamDefaultReader(this);
    }
    cancel(reason) {
      if (this.locked) return Promise.reject(new TypeError('Cannot cancel a locked stream'));
      return this._cancel(reason);
    }
    pipeTo(dest, options = {}) {
      if (this.locked || dest.locked) return Promise.reject(new TypeError('Cannot pipe a locked stream'));
      const { preventClose, preventAbort, preventCancel, signal } = options || {};
      const reader = this.getReader();
      const writer = dest.getWriter();
      return new Promise((resolve, reject) => {
        let done = false;
        const finish = (err, isError) => {
          if (done) return;
          done = true;
          reader.releaseLock();
          writer.releaseLock();
          isError ? reject(err) : resolve();
        };
        if (signal) {
          const abort = () => {
            const reason = signal.reason ?? new DOMException('The operation was aborted.', 'AbortError');
            Promise.all([preventAbort ? null : dest.abort(reason), preventCancel ? null : this.cancel(reason)].map((p) => p && Promise.resolve(p).catch(() => {})))
              .then(() => finish(reason, true));
          };
          if (signal.aborted) { abort(); return; }
          signal.addEventListener('abort', abort);
        }
        const step = () => {
          reader.read().then(({ value, done: end }) => {
            if (done) return;
            if (end) {
              if (preventClose) return finish();
              writer.close().then(() => finish(), (e) => finish(e, true));
              return;
            }
            writer.write(value).then(step, (e) => {
              if (!preventCancel) reader.cancel(e).catch(() => {});
              finish(e, true);
            });
          }, (e) => {
            if (!preventAbort) writer.abort(e).catch(() => {});
            finish(e, true);
          });
        };
        step();
      });
    }
    pipeThrough(transform, options) {
      this.pipeTo(transform.writable, options).catch(() => {});
      return transform.readable;
    }
    tee() {
      const reader = this.getReader();
      let reading = false;
      const controllers = [];
      const canceled = [false, false];
      const pull = () => {
        if (reading) return;
        reading = true;
        return reader.read().then(({ value, done }) => {
          reading = false;
          controllers.forEach((c, i) => { if (!canceled[i]) { try { done ? c.close() : c.enqueue(value); } catch {} } });
        }, (e) => controllers.forEach((c) => c.error(e)));
      };
      const branch = (i) => new ReadableStream({
        start(c) { controllers[i] = c; },
        pull,
        cancel(reason) { canceled[i] = true; if (canceled[0] && canceled[1]) return reader.cancel(reason); },
      });
      return [branch(0), branch(1)];
    }
    values(options = {}) {
      const reader = this.getReader();
      const preventCancel = !!(options && options.preventCancel);
      return {
        next: () => reader.read().then((r) => { if (r.done) reader.releaseLock(); return r; }),
        return: (value) => {
          const p = preventCancel ? Promise.resolve() : reader.cancel(value);
          return p.then(() => { reader.releaseLock(); return { value, done: true }; });
        },
        [Symbol.asyncIterator]() { return this; },
      };
    }
    [Symbol.asyncIterator](options) { return this.values(options); }
    static from(iterable) {
      const it = iterable[Symbol.asyncIterator] ? iterable[Symbol.asyncIterator]() : iterable[Symbol.iterator]();
      return new ReadableStream({
        pull(c) { return Promise.resolve(it.next()).then(({ value, done }) => (done ? c.close() : Promise.resolve(value).then((v) => c.enqueue(v)))); },
        cancel(reason) { return it.return && it.return(reason); },
      }, { highWaterMark: 0 });
    }
  }

  class WritableStreamDefaultWriter {
    constructor(stream) {
      if (stream._writer) throw new TypeError('WritableStream is locked');
      Object.defineProperty(this, '_s', { value: stream, writable: true });
      stream._writer = this;
    }
    get closed() { return this._s ? this._s._closed.promise : Promise.reject(new TypeError('Writer was released')); }
    get ready() { return this._s ? this._s._readyPromise() : Promise.reject(new TypeError('Writer was released')); }
    get desiredSize() { return this._s ? this._s._desiredSize() : null; }
    write(chunk) { return this._s ? this._s._write(chunk) : Promise.reject(new TypeError('Writer was released')); }
    close() { return this._s ? this._s._closeStream() : Promise.reject(new TypeError('Writer was released')); }
    abort(reason) { return this._s ? this._s._abort(reason) : Promise.reject(new TypeError('Writer was released')); }
    releaseLock() { if (this._s) { this._s._writer = null; this._s = null; } }
  }

  class WritableStream {
    constructor(sink = {}, strategy = {}) {
      sink = sink ?? {};
      const abortController = new AbortController();
      Object.defineProperties(this, {
        _sink: { value: sink }, _queue: { value: [] }, _state: { value: 'writable', writable: true }, _writer: { value: null, writable: true },
        _writing: { value: false, writable: true }, _started: { value: false, writable: true }, _closeRequest: { value: null, writable: true },
        _storedError: { value: undefined, writable: true }, _closed: { value: deferred() }, _hwm: { value: strategy.highWaterMark ?? 1 },
        _abortController: { value: abortController },
      });
      this._closed.promise.catch(() => {});
      const controller = { error: (e) => this._error(e), signal: abortController.signal };
      Object.defineProperty(this, '_controller', { value: controller });
      let started;
      try { started = sink.start ? sink.start.call(sink, controller) : undefined; } catch (e) { this._error(e); }
      Promise.resolve(started).then(() => { this._started = true; this._advance(); }, (e) => this._error(e));
    }
    get locked() { return !!this._writer; }
    getWriter() { return new WritableStreamDefaultWriter(this); }
    close() { return this.locked ? Promise.reject(new TypeError('Cannot close a locked stream')) : this._closeStream(); }
    abort(reason) { return this.locked ? Promise.reject(new TypeError('Cannot abort a locked stream')) : this._abort(reason); }
    _desiredSize() { return this._state === 'errored' ? null : this._state === 'closed' ? 0 : this._hwm - this._queue.length - (this._writing ? 1 : 0); }
    _readyPromise() { return this._state === 'errored' ? Promise.reject(this._storedError) : Promise.resolve(); }
    _write(chunk) {
      if (this._state !== 'writable' || this._closeRequest) return Promise.reject(this._state === 'errored' ? this._storedError : new TypeError('Cannot write to a closing or closed stream'));
      const d = deferred();
      this._queue.push({ chunk, d });
      this._advance();
      return d.promise;
    }
    _advance() {
      if (!this._started || this._writing || this._state !== 'writable') return;
      if (!this._queue.length) {
        if (this._closeRequest) this._finishClose();
        return;
      }
      this._writing = true;
      const { chunk, d } = this._queue.shift();
      Promise.resolve()
        .then(() => this._sink.write && this._sink.write.call(this._sink, chunk, this._controller))
        .then(() => { this._writing = false; d.resolve(); this._advance(); }, (e) => { this._writing = false; d.reject(e); this._error(e); });
    }
    _closeStream() {
      if (this._state !== 'writable' || this._closeRequest) return Promise.reject(new TypeError('The stream is closing or closed'));
      this._closeRequest = deferred();
      this._advance();
      return this._closeRequest.promise;
    }
    _finishClose() {
      const req = this._closeRequest;
      Promise.resolve()
        .then(() => this._sink.close && this._sink.close.call(this._sink))
        .then(() => { this._state = 'closed'; this._closed.resolve(); req.resolve(); }, (e) => { this._error(e); req.reject(e); });
    }
    _error(e) {
      if (this._state !== 'writable') return;
      this._state = 'errored';
      this._storedError = e;
      for (const { d } of this._queue.splice(0)) d.reject(e);
      this._closed.reject(e);
      if (this._closeRequest) this._closeRequest.reject(e);
    }
    _abort(reason) {
      if (this._state === 'closed' || this._state === 'errored') return Promise.resolve();
      this._abortController.abort(reason);
      this._error(reason);
      return Promise.resolve().then(() => this._sink.abort && this._sink.abort.call(this._sink, reason)).then(() => undefined);
    }
  }

  class TransformStream {
    constructor(transformer = {}, writableStrategy = {}, readableStrategy = {}) {
      transformer = transformer ?? {};
      let readController;
      const readable = new ReadableStream({ start(c) { readController = c; } }, readableStrategy);
      let writable;
      const controller = {
        enqueue: (chunk) => readController.enqueue(chunk),
        error: (e) => { readController.error(e); writable._error(e); },
        terminate: () => { try { readController.close(); } catch {} writable._error(new TypeError('The transform stream has been terminated')); },
        get desiredSize() { return readController.desiredSize; },
      };
      writable = new WritableStream({
        start: () => transformer.start && transformer.start.call(transformer, controller),
        write: (chunk) => (transformer.transform ? transformer.transform.call(transformer, chunk, controller) : controller.enqueue(chunk)),
        close: () => Promise.resolve(transformer.flush && transformer.flush.call(transformer, controller)).then(() => { try { readController.close(); } catch {} }),
        abort: (reason) => readController.error(reason),
      }, writableStrategy);
      Object.defineProperties(this, { readable: { value: readable, enumerable: true }, writable: { value: writable, enumerable: true } });
    }
  }

  class TextEncoderStream extends TransformStream {
    constructor() {
      const encoder = new TextEncoder();
      super({ transform(chunk, c) { const s = String(chunk); if (s) c.enqueue(encoder.encode(s)); } });
    }
    get encoding() { return 'utf-8'; }
  }

  class TextDecoderStream extends TransformStream {
    constructor(label = 'utf-8', options = {}) {
      const decoder = new TextDecoder(label, options);
      super({
        transform(chunk, c) { const s = decoder.decode(chunk, { stream: true }); if (s) c.enqueue(s); },
        flush(c) { const s = decoder.decode(); if (s) c.enqueue(s); },
      });
      Object.defineProperty(this, '_decoder', { value: decoder });
    }
    get encoding() { return this._decoder.encoding; }
  }

  // Byte-stream sources a body or blob gives: one chunk, then done
  const bytesStream = (getBytes) => new ReadableStream({
    pull(c) {
      const bytes = getBytes();
      if (bytes && bytes.byteLength) c.enqueue(bytes);
      c.close();
    },
  }, { highWaterMark: 0 });

  // ---- MessageChannel ----

  class MessagePort extends EventTargetCtor {
    constructor() {
      super();
      Object.defineProperties(this, { _other: { value: null, writable: true }, _closed: { value: false, writable: true }, _onmessage: { value: null, writable: true } });
    }
    get onmessage() { return this._onmessage; }
    set onmessage(f) { this._onmessage = typeof f === 'function' ? f : null; }
    postMessage(data) {
      const target = this._other;
      if (!target || this._closed) return;
      const value = structuredClone(data);
      setTimeout(() => {
        if (target._closed) return;
        const ev = Object.assign(new Event('message'), { data: value, ports: [], origin: '', lastEventId: '' });
        target.dispatchEvent(ev);
        if (target._onmessage) { try { target._onmessage.call(target, ev); } catch (e) { reportError(e); } }
      }, 0);
    }
    start() {}
    close() { this._closed = true; }
  }
  class MessageChannel {
    constructor() {
      const port1 = new MessagePort(), port2 = new MessagePort();
      port1._other = port2;
      port2._other = port1;
      Object.defineProperties(this, { port1: { value: port1, enumerable: true }, port2: { value: port2, enumerable: true } });
    }
  }

  class FormData {
    constructor(form) {
      this._list = [];
      if (!form || typeof form.querySelectorAll !== 'function') return;
      for (const el of form.querySelectorAll('input[name], select[name], textarea[name], button[name]')) {
        if (el.disabled) continue;
        const type = (el.getAttribute('type') || '').toLowerCase();
        if (el.localName === 'button' || ['submit', 'button', 'reset', 'image', 'file'].includes(type)) continue;
        if ((type === 'checkbox' || type === 'radio') && !el.checked) continue;
        this._list.push([el.getAttribute('name'), el.value]);
      }
    }
    _entry(v, filename) {
      if (v instanceof Blob) return filename !== undefined || !(v instanceof File) ? new File([v], filename ?? 'blob', { type: v.type }) : v;
      return String(v);
    }
    append(name, value, filename) { this._list.push([String(name), this._entry(value, filename)]); }
    set(name, value, filename) {
      name = String(name);
      const i = this._list.findIndex(([n]) => n === name);
      const e = [name, this._entry(value, filename)];
      if (i < 0) this._list.push(e);
      else { this._list[i] = e; this._list = this._list.filter(([n], j) => n !== name || j === i); }
    }
    get(name) { const e = this._list.find(([n]) => n === String(name)); return e ? e[1] : null; }
    getAll(name) { return this._list.filter(([n]) => n === String(name)).map(e => e[1]); }
    has(name) { return this._list.some(([n]) => n === String(name)); }
    delete(name) { this._list = this._list.filter(([n]) => n !== String(name)); }
    forEach(f, thisArg) { for (const [n, v] of this._list) f.call(thisArg, v, n, this); }
    keys() { return this._list.map(e => e[0])[Symbol.iterator](); }
    values() { return this._list.map(e => e[1])[Symbol.iterator](); }
    entries() { return this._list.map(e => [e[0], e[1]])[Symbol.iterator](); }
    [Symbol.iterator]() { return this.entries(); }
    // multipart/form-data encoding
    _encode() {
      const boundary = '----fOSFormBoundary' + Math.random().toString(36).slice(2) + Math.random().toString(36).slice(2);
      const esc = s => String(s).replace(/"/g, '%22').replace(/\r/g, '%0D').replace(/\n/g, '%0A');
      const parts = [];
      for (const [name, value] of this._list) {
        parts.push('--' + boundary + '\r\nContent-Disposition: form-data; name="' + esc(name) + '"');
        if (value instanceof File) {
          parts.push('; filename="' + esc(value.name) + '"\r\nContent-Type: ' + (value.type || 'application/octet-stream') + '\r\n\r\n', value, '\r\n');
        } else {
          parts.push('\r\n\r\n' + value.replace(/\r?\n/g, '\r\n') + '\r\n');
        }
      }
      parts.push('--' + boundary + '--\r\n');
      return { data: new Blob(parts)._buf, type: 'multipart/form-data; boundary=' + boundary };
    }
  }

  const TOKEN = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
  class Headers {
    constructor(init) {
      this._list = [];
      if (init == null) return;
      if (init instanceof Headers) { for (const [k, v] of init._list) this._list.push([k, v]); return; }
      if (typeof init !== 'object') throw new TypeError("Failed to construct 'Headers': The provided value is not of type 'HeadersInit'");
      if (Symbol.iterator in init) {
        for (const pair of init) {
          const p = [...pair];
          if (p.length !== 2) throw new TypeError("Failed to construct 'Headers': Invalid value");
          this.append(p[0], p[1]);
        }
      } else {
        for (const k of Object.keys(init)) this.append(k, init[k]);
      }
    }
    _name(n) {
      n = String(n);
      if (!TOKEN.test(n)) throw new TypeError(`'${n}' is not a valid HTTP header field name.`);
      return n.toLowerCase();
    }
    _value(v) {
      v = String(v).replace(/^[\t\n\r ]+|[\t\n\r ]+$/g, '');
      if (/[\0\r\n]/.test(v)) throw new TypeError(`'${v}' is not a valid HTTP header field value.`);
      return v;
    }
    append(n, v) { this._list.push([this._name(n), this._value(v)]); }
    delete(n) { n = this._name(n); this._list = this._list.filter(([k]) => k !== n); }
    get(n) {
      n = this._name(n);
      const vals = this._list.filter(([k]) => k === n).map(e => e[1]);
      return vals.length ? vals.join(', ') : null;
    }
    getSetCookie() { return []; }
    has(n) { n = this._name(n); return this._list.some(([k]) => k === n); }
    set(n, v) { n = this._name(n); v = this._value(v); this._list = this._list.filter(([k]) => k !== n); this._list.push([n, v]); }
    _sorted() { return [...new Set(this._list.map(e => e[0]))].sort().map(k => [k, this.get(k)]); }
    forEach(f, thisArg) { for (const [k, v] of this._sorted()) f.call(thisArg, v, k, this); }
    keys() { return this._sorted().map(e => e[0])[Symbol.iterator](); }
    values() { return this._sorted().map(e => e[1])[Symbol.iterator](); }
    entries() { return this._sorted()[Symbol.iterator](); }
    [Symbol.iterator]() { return this.entries(); }
  }

  // A body given to Request, Response or XMLHttpRequest: its bytes (or
  // string) and the content type it implies
  function extractBody(body) {
    if (body == null) return { data: null, type: null };
    if (typeof body === 'string') return { data: body, type: 'text/plain;charset=UTF-8' };
    if (body instanceof URLSearchParams) return { data: body.toString(), type: 'application/x-www-form-urlencoded;charset=UTF-8' };
    if (body instanceof FormData) return body._encode();
    if (body instanceof Blob) return { data: body._buf, type: body.type || null };
    if (body instanceof ArrayBuffer || ArrayBuffer.isView(body)) return { data: body, type: null };
    return { data: String(body), type: 'text/plain;charset=UTF-8' };
  }

  const bodyMixin = {
    _consume() {
      if (this.bodyUsed) return Promise.reject(new TypeError('Failed to execute: body stream already read'));
      this.bodyUsed = true;
      return Promise.resolve(this._body);
    },
    text() { return this._consume().then(b => b == null ? '' : typeof b === 'string' ? b : __fosDecode(b, 'utf-8')); },
    json() { return this.text().then(t => JSON.parse(t)); },
    arrayBuffer() { return this._consume().then(toArrayBuffer); },
    bytes() { return this.arrayBuffer().then(b => new Uint8Array(b)); },
    blob() { return this.arrayBuffer().then(b => new Blob([b], { type: this.headers.get('content-type') || '' })); },
    formData() {
      return this.text().then(t => {
        const fd = new FormData();
        for (const [k, v] of new URLSearchParams(t)) fd.append(k, v);
        return fd;
      });
    },
    get body() {
      if (this._body == null) return null;
      if (!this._stream) {
        this._stream = bytesStream(() => {
          if (this.bodyUsed) return null;
          this.bodyUsed = true;
          return new Uint8Array(toArrayBuffer(this._body));
        });
      }
      return this._stream;
    },
  };

  const METHODS = /^(delete|get|head|options|post|put|patch)$/i;
  class Request {
    constructor(input, init = {}) {
      init = init || {};
      const src = input instanceof Request ? input : null;
      this.url = src ? src.url : new URL(String(input), document.baseURI).href;
      let method = String(init.method ?? (src ? src.method : 'GET'));
      if (!TOKEN.test(method)) throw new TypeError(`'${method}' is not a valid HTTP method.`);
      this.method = METHODS.test(method) ? method.toUpperCase() : method;
      this.headers = new Headers(init.headers ?? (src ? src.headers : undefined));
      this.mode = init.mode ?? (src ? src.mode : 'cors');
      this.credentials = init.credentials ?? (src ? src.credentials : 'same-origin');
      this.redirect = init.redirect ?? (src ? src.redirect : 'follow');
      this.cache = init.cache ?? (src ? src.cache : 'default');
      this.referrer = src ? src.referrer : 'about:client';
      this.referrerPolicy = init.referrerPolicy ?? (src ? src.referrerPolicy : '');
      this.integrity = init.integrity ?? (src ? src.integrity : '');
      this.keepalive = !!(init.keepalive ?? (src ? src.keepalive : false));
      this.signal = init.signal ?? (src ? src.signal : new AbortController().signal);
      this.destination = '';
      this.bodyUsed = false;
      if (init.body != null) {
        if (this.method === 'GET' || this.method === 'HEAD') throw new TypeError('Request with GET/HEAD method cannot have body.');
        const b = extractBody(init.body);
        this._body = b.data;
        if (b.type && !this.headers.has('content-type')) this.headers.set('content-type', b.type);
      } else {
        this._body = src ? src._body : null;
      }
    }
    clone() {
      if (this.bodyUsed) throw new TypeError("Failed to execute 'clone' on 'Request': Request body is already used");
      return new Request(this);
    }
  }
  define(Request.prototype, bodyMixin);

  class Response {
    constructor(body = null, init = {}) {
      init = init || {};
      const status = init.status === undefined ? 200 : Number(init.status);
      if (!(status >= 200 && status <= 599)) throw new RangeError(`Failed to construct 'Response': The status provided (${status}) is outside the range [200, 599].`);
      this.status = status;
      this.statusText = init.statusText === undefined ? '' : String(init.statusText);
      this.headers = new Headers(init.headers);
      this.type = 'default';
      this.url = '';
      this.redirected = false;
      this.bodyUsed = false;
      const b = extractBody(body);
      this._body = b.data;
      if (b.type && !this.headers.has('content-type')) this.headers.set('content-type', b.type);
    }
    get ok() { return this.status >= 200 && this.status < 300; }
    clone() {
      if (this.bodyUsed) throw new TypeError("Failed to execute 'clone' on 'Response': Response body is already used");
      const r = Object.create(Response.prototype);
      Object.assign(r, this, { headers: new Headers(this.headers), bodyUsed: false, _stream: undefined });
      return r;
    }
    static error() {
      const r = Object.create(Response.prototype);
      Object.assign(r, { status: 0, statusText: '', headers: new Headers(), type: 'error', url: '', redirected: false, bodyUsed: false, _body: null });
      return r;
    }
    static redirect(url, status = 302) {
      if (![301, 302, 303, 307, 308].includes(status)) throw new RangeError('Invalid status code');
      return new Response(null, { status, headers: { location: new URL(url, document.baseURI).href } });
    }
    static json(data, init = {}) {
      const headers = new Headers(init.headers);
      if (!headers.has('content-type')) headers.set('content-type', 'application/json');
      return new Response(JSON.stringify(data), { ...init, headers });
    }
  }
  define(Response.prototype, bodyMixin);

  // A Response from what __fosFetch delivers
  function hostResponse(r) {
    const res = Object.create(Response.prototype);
    const headers = new Headers();
    headers._list = r.headers;
    Object.assign(res, { status: r.status, statusText: r.statusText, url: r.url, redirected: r.redirected,
      type: r.type, headers, bodyUsed: false, _body: r.body });
    return res;
  }

  function fetch(input, init) {
    return new Promise((resolve, reject) => {
      const req = new Request(input, init);
      const signal = req.signal;
      if (signal && signal.aborted) { reject(signal.reason); return; }
      let settled = false;
      const onAbort = () => { if (!settled) { settled = true; reject(signal.reason); } };
      if (signal) signal.addEventListener('abort', onAbort);
      __fosFetch(req.method, req.url, [...req.headers], req._body, req.mode, req.credentials, req.redirect, (err, r) => {
        if (signal) signal.removeEventListener('abort', onAbort);
        if (settled) return;
        settled = true;
        if (err !== null) reject(new TypeError('Failed to fetch'));
        else resolve(hostResponse(r));
      });
    });
  }

  class XMLHttpRequestEventTarget extends EventTargetCtor {}
  class XMLHttpRequestUpload extends XMLHttpRequestEventTarget {}
  class XMLHttpRequest extends XMLHttpRequestEventTarget {
    constructor() {
      super();
      this.upload = new XMLHttpRequestUpload();
      this.timeout = 0;
      this.withCredentials = false;
      this._responseType = '';
      this._gen = 0;
      this._clear();
      this.readyState = 0;
    }
    _clear() {
      this.status = 0;
      this.statusText = '';
      this.responseURL = '';
      this._resp = null;
      this._text = null;
      this._json = undefined;
      this._sent = false;
    }
    get responseType() { return this._responseType; }
    set responseType(t) {
      if (this.readyState >= 3) throw new DOMException('The response type cannot be set if the object\'s state is LOADING or DONE.', 'InvalidStateError');
      if (['', 'text', 'json', 'arraybuffer', 'blob', 'document'].includes(t)) this._responseType = t;
    }
    open(method, url, async = true) {
      method = String(method);
      if (!TOKEN.test(method)) throw new DOMException(`'${method}' is not a valid HTTP method.`, 'SyntaxError');
      let abs;
      try { abs = new URL(String(url), document.baseURI).href; } catch { throw new DOMException(`Invalid URL`, 'SyntaxError'); }
      this._gen++;
      clearTimeout(this._timer);
      this._clear();
      this._method = METHODS.test(method) ? method.toUpperCase() : method;
      this._url = abs;
      this._async = async !== false;
      this._headers = new Headers();
      this._set(1);
    }
    _set(state) { this.readyState = state; this.dispatchEvent(new Event('readystatechange')); }
    _progress(type, target = this, loaded = 0, total = 0) {
      target.dispatchEvent(new ProgressEvent(type, { lengthComputable: total > 0, loaded, total }));
    }
    setRequestHeader(name, value) {
      if (this.readyState !== 1 || this._sent) throw new DOMException("Failed to execute 'setRequestHeader': The object's state must be OPENED.", 'InvalidStateError');
      const n = String(name).toLowerCase();
      const prev = this._headers.get(n);
      this._headers.set(n, prev === null ? value : prev + ', ' + value);
    }
    send(body = null) {
      if (this.readyState !== 1 || this._sent) throw new DOMException("Failed to execute 'send': The object's state must be OPENED.", 'InvalidStateError');
      if (this._method === 'GET' || this._method === 'HEAD') body = null;
      const b = extractBody(body);
      if (b.type && !this._headers.has('content-type')) this._headers.set('content-type', b.type);
      this._sent = true;
      const credentials = this.withCredentials ? 'include' : 'same-origin';
      if (!this._async) {
        let r;
        try {
          r = __fosFetch(this._method, this._url, [...this._headers], b.data, 'cors', credentials, 'follow');
        } catch (e) {
          this._clear();
          this._set(4);
          throw new DOMException("Failed to execute 'send' on 'XMLHttpRequest': Failed to load '" + this._url + "'.", 'NetworkError');
        }
        this._receive(r);
        this._set(4);
        const n = r.body.byteLength;
        this._progress('load', this, n, n);
        this._progress('loadend', this, n, n);
        return;
      }
      const gen = this._gen;
      this._progress('loadstart');
      if (b.data != null) this._progress('loadstart', this.upload);
      if (this.timeout > 0) {
        this._timer = setTimeout(() => { if (gen === this._gen) { this._gen++; this._fail('timeout'); } }, this.timeout);
      }
      __fosFetch(this._method, this._url, [...this._headers], b.data, 'cors', credentials, 'follow', (err, r) => {
        if (gen !== this._gen) return; // aborted, timed out or reopened
        clearTimeout(this._timer);
        if (err !== null) { this._fail('error'); return; }
        if (b.data != null) { this._progress('load', this.upload); this._progress('loadend', this.upload); }
        this._receive(r);
        this._set(2);
        const n = r.body.byteLength;
        this._set(3);
        this._progress('progress', this, n, n);
        if (gen !== this._gen) return;
        this._set(4);
        this._progress('load', this, n, n);
        this._progress('loadend', this, n, n);
      });
    }
    _receive(r) {
      this._resp = r;
      this.status = r.status;
      this.statusText = r.statusText;
      this.responseURL = r.url;
    }
    _fail(kind) {
      this._clear();
      this._set(4);
      this._progress(kind);
      this._progress('loadend');
    }
    abort() {
      const active = (this.readyState === 1 && this._sent) || this.readyState === 2 || this.readyState === 3;
      this._gen++;
      clearTimeout(this._timer);
      if (active) this._fail('abort');
      if (this.readyState === 4) { this._clear(); this.readyState = 0; }
    }
    getResponseHeader(name) {
      if (!this._resp) return null;
      const n = String(name).toLowerCase();
      const vals = this._resp.headers.filter(([k]) => k === n).map(e => e[1]);
      return vals.length ? vals.join(', ') : null;
    }
    getAllResponseHeaders() {
      if (!this._resp) return '';
      const h = new Headers();
      h._list = this._resp.headers;
      let out = '';
      for (const [k, v] of h) out += k + ': ' + v + '\r\n';
      return out;
    }
    overrideMimeType(mime) { this._mime = String(mime); }
    _charset() {
      const ct = this._mime || this.getResponseHeader('content-type') || '';
      const m = /charset\s*=\s*"?([^";\s]+)/i.exec(ct);
      return m ? m[1] : 'utf-8';
    }
    get responseText() {
      if (this._responseType !== '' && this._responseType !== 'text') {
        throw new DOMException("The value is only accessible if the object's 'responseType' is '' or 'text'.", 'InvalidStateError');
      }
      if (!this._resp || this.readyState < 3) return '';
      if (this._text === null) {
        try { this._text = __fosDecode(this._resp.body, this._charset()); } catch { this._text = __fosDecode(this._resp.body, 'utf-8'); }
      }
      return this._text;
    }
    get response() {
      const t = this._responseType;
      if (t === '' || t === 'text') return this.responseText;
      if (this.readyState !== 4 || !this._resp) return null;
      if (t === 'json') {
        if (this._json === undefined) {
          try { this._json = JSON.parse(__fosDecode(this._resp.body, 'utf-8')); } catch { this._json = null; }
        }
        return this._json;
      }
      if (t === 'arraybuffer') return this._resp.body;
      if (t === 'blob') return new Blob([this._resp.body], { type: this.getResponseHeader('content-type') || '' });
      return null;
    }
    get responseXML() { return null; }
  }
  for (const [k, v] of [['UNSENT', 0], ['OPENED', 1], ['HEADERS_RECEIVED', 2], ['LOADING', 3], ['DONE', 4]]) {
    Object.defineProperty(XMLHttpRequest, k, { value: v });
    Object.defineProperty(XMLHttpRequest.prototype, k, { value: v });
  }

  const noopObserver = class { constructor(cb) { this._cb = cb; } observe() {} unobserve() {} disconnect() {} takeRecords() { return []; } };

  // ---- performance timeline (User Timing) ----

  class PerformanceEntry {
    constructor(name, entryType, startTime, duration) {
      Object.assign(this, { name: String(name), entryType, startTime, duration });
    }
    toJSON() { return { ...this }; }
  }
  class PerformanceMark extends PerformanceEntry {
    constructor(name, options = {}) {
      super(name, 'mark', options.startTime ?? performance.now(), 0);
      this.detail = options.detail ?? null;
    }
  }
  class PerformanceMeasure extends PerformanceEntry {}
  const perfEntries = [];
  const perfObservers = new Set();
  const navigationEntry = new PerformanceEntry(document.URL, 'navigation', 0, 0);
  Object.assign(navigationEntry, { type: 'navigate', redirectCount: 0, domInteractive: 0, domContentLoadedEventStart: 0, domContentLoadedEventEnd: 0, loadEventStart: 0, loadEventEnd: 0 });
  function recordEntry(entry) {
    perfEntries.push(entry);
    for (const o of perfObservers) {
      if (o._types.has(entry.entryType)) {
        o._queue.push(entry);
        scheduleObserver(o);
      }
    }
    return entry;
  }
  // Observers get their entries in a later task, batched
  function scheduleObserver(o) {
    if (o._scheduled) return;
    o._scheduled = true;
    setTimeout(() => {
      o._scheduled = false;
      const list = o.takeRecords();
      if (list.length) o._cb(new PerformanceObserverEntryList(list), o);
    }, 0);
  }
  function entriesOf(type) {
    return type === 'navigation' ? [navigationEntry] : perfEntries.filter(e => e.entryType === type);
  }
  function markTime(v) {
    if (v === undefined) return undefined;
    if (typeof v === 'number') return v;
    if (v === 'navigationStart' || v === 'fetchStart') return 0;
    const marks = perfEntries.filter(e => e.entryType === 'mark' && e.name === String(v));
    if (!marks.length) throw new DOMException(`The mark '${v}' does not exist.`, 'SyntaxError');
    return marks[marks.length - 1].startTime;
  }
  const timeOrigin = Date.now() - performance.now();
  Object.assign(performance, {
    timeOrigin,
    timing: { navigationStart: timeOrigin, fetchStart: timeOrigin, responseEnd: timeOrigin, domLoading: timeOrigin, domInteractive: 0, domContentLoadedEventEnd: 0, loadEventEnd: 0 },
    navigation: { type: 0, redirectCount: 0 },
    eventCounts: new Map(),
    mark(name, options) { return recordEntry(new PerformanceMark(name, options)); },
    measure(name, start, end) {
      let startTime, endTime, detail = null;
      if (start && typeof start === 'object') {
        detail = start.detail ?? null;
        startTime = markTime(start.start);
        endTime = markTime(start.end);
        if (start.duration !== undefined) {
          if (startTime === undefined) startTime = endTime - start.duration;
          else endTime = startTime + start.duration;
        }
      } else {
        startTime = markTime(start);
        endTime = markTime(end);
      }
      startTime ??= 0;
      endTime ??= performance.now();
      const m = new PerformanceMeasure(name, 'measure', startTime, endTime - startTime);
      m.detail = detail;
      return recordEntry(m);
    },
    getEntries() { return [navigationEntry, ...perfEntries]; },
    getEntriesByType(type) { return entriesOf(String(type)); },
    getEntriesByName(name, type) {
      return (type ? entriesOf(String(type)) : this.getEntries()).filter(e => e.name === String(name));
    },
    clearMarks(name) { removeEntries('mark', name); },
    clearMeasures(name) { removeEntries('measure', name); },
    clearResourceTimings() {},
    setResourceTimingBufferSize() {},
    toJSON() { return { timeOrigin, timing: this.timing, navigation: this.navigation }; },
  });
  function removeEntries(type, name) {
    for (let i = perfEntries.length - 1; i >= 0; i--) {
      if (perfEntries[i].entryType === type && (name === undefined || perfEntries[i].name === String(name))) perfEntries.splice(i, 1);
    }
  }
  class PerformanceObserverEntryList {
    constructor(list) { this._list = list; }
    getEntries() { return this._list.slice(); }
    getEntriesByType(t) { return this._list.filter(e => e.entryType === t); }
    getEntriesByName(n, t) { return this._list.filter(e => e.name === n && (!t || e.entryType === t)); }
  }
  class PerformanceObserver {
    constructor(callback) {
      if (typeof callback !== 'function') throw new TypeError("Failed to construct 'PerformanceObserver': The callback provided as parameter 1 is not a function.");
      Object.assign(this, { _cb: callback, _types: new Set(), _queue: [], _scheduled: false });
    }
    observe(options = {}) {
      const types = options.entryTypes || (options.type ? [options.type] : []);
      for (const t of types) this._types.add(String(t));
      perfObservers.add(this);
      if (options.buffered) {
        for (const t of types) for (const e of entriesOf(String(t))) this._queue.push(e);
        if (this._queue.length) scheduleObserver(this);
      }
    }
    disconnect() { perfObservers.delete(this); this._queue = []; }
    takeRecords() { return this._queue.splice(0); }
    static get supportedEntryTypes() { return ['mark', 'measure', 'navigation']; }
  }

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
    get innerWidth() { return __fosViewport()[0]; }, get innerHeight() { return __fosViewport()[1]; },
    get outerWidth() { return __fosViewport()[0]; }, get outerHeight() { return __fosViewport()[1]; },
    devicePixelRatio: 1,
    get scrollX() { return __fosViewport()[2]; }, get scrollY() { return __fosViewport()[3]; },
    get pageXOffset() { return __fosViewport()[2]; }, get pageYOffset() { return __fosViewport()[3]; },
    screenX: 0, screenY: 0,
    screen: { width: 1920, height: 1080, availWidth: 1920, availHeight: 1080, colorDepth: 24, pixelDepth: 24 },
    navigator: {
      userAgent: 'Mozilla/5.0 (X11; Linux x86_64) fOS/0.1 (KHTML, like Gecko)',
      appName: 'Netscape', appVersion: '5.0', appCodeName: 'Mozilla', product: 'Gecko',
      platform: 'Linux x86_64', vendor: '', language: 'en-US', languages: ['en-US', 'en'],
      onLine: true, cookieEnabled: true, doNotTrack: null, hardwareConcurrency: 4, maxTouchPoints: 0,
      webdriver: false, javaEnabled: () => false,
      sendBeacon(url, data) {
        fetch(url, { method: 'POST', body: data ?? null, mode: 'no-cors', credentials: 'include', keepalive: true }).catch(() => {});
        return true;
      },
      clipboard: { writeText: () => Promise.resolve(), readText: () => Promise.resolve('') },
    },
    history: new History(),
    localStorage: new Storage(),
    sessionStorage: new Storage(),
    Event, CustomEvent, UIEvent, MouseEvent, KeyboardEvent, FocusEvent, InputEvent, ErrorEvent,
    PointerEvent: MouseEvent, TouchEvent: UIEvent, WheelEvent: MouseEvent, AnimationEvent: Event,
    TransitionEvent: Event, PopStateEvent, HashChangeEvent, MessageEvent: Event,
    ProgressEvent,
    URL, URLSearchParams, Storage, DOMTokenList, AbortController, AbortSignal, DOMException,
    EventTarget: EventTargetCtor,
    fetch, Headers, Request, Response, Blob, File, FormData, ReadableStream,
    ReadableStreamDefaultReader, ReadableStreamDefaultController, WritableStream, WritableStreamDefaultWriter,
    TransformStream, TextEncoderStream, TextDecoderStream, MessageChannel, MessagePort,
    XMLHttpRequest, XMLHttpRequestUpload, XMLHttpRequestEventTarget,
    TextEncoder, TextDecoder,
    MutationObserver: noopObserver, IntersectionObserver: noopObserver, ResizeObserver: noopObserver,
    PerformanceObserver, PerformanceEntry, PerformanceMark, PerformanceMeasure,
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
    scrollTo(x, y) { __fosScrollTo(typeof x === 'object' && x ? (+x.top || 0) : (+y || 0)); },
    scroll(x, y) { global.scrollTo(x, y); },
    scrollBy(x, y) { __fosScrollTo(__fosViewport()[3] + (typeof x === 'object' && x ? (+x.top || 0) : (+y || 0))); },
    focus() {}, blur() {}, print() {}, stop() {},
    alert(msg) { console.info('[alert]', msg); },
    confirm(msg) { console.info('[confirm]', msg); return false; },
    prompt(msg) { console.info('[prompt]', msg); return null; },
    open() { return null; }, close() {},
    postMessage(data) { setTimeout(() => global.dispatchEvent(Object.assign(new Event('message'), { data, origin: global.location.origin, source: global })), 0); },
    structuredClone(v) { return v === undefined ? v : JSON.parse(JSON.stringify(v)); },
    reportError,
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
