//#region src/http.ts
var e = 15e3;
function t(e = globalThis.navigator?.userAgent, t = globalThis.location?.protocol) {
	let n = e ?? "";
	return /^((?!chrome|android).)*safari/i.test(n) && t === "https:" ? "https://127.0.0.1:9101/" : "http://127.0.0.1:9100/";
}
function n(e) {
	return e instanceof Error ? e.name === "AbortError" ? "Request timed out" : e.message || String(e) : String(e);
}
function r(e, t = {}) {
	return {
		...t,
		debug: e
	};
}
function i(e) {
	return e.length <= 512 ? `body=${JSON.stringify(e)}` : `body=${JSON.stringify(e.slice(0, 512))} truncated ${e.length} chars`;
}
function a(e) {
	return new TextEncoder().encode(e).length;
}
function o(e) {
	if (e == null) return "";
	if (typeof e == "string") return ` ${i(e)}`;
	if (typeof URLSearchParams < "u" && e instanceof URLSearchParams) return ` ${i(e.toString())}`;
	if (typeof Blob < "u" && e instanceof Blob) return ` body=${e.size} bytes`;
	if (typeof FormData < "u" && e instanceof FormData) {
		let t = 0;
		for (let n of e.values()) t += typeof n == "string" ? a(n) : n.size;
		return ` body=${t} bytes`;
	}
	return e instanceof ArrayBuffer || ArrayBuffer.isView(e) ? ` body=${e.byteLength} bytes` : " body=(unlogged)";
}
function s(e, t, n, r, i) {
	e && console.log(`BrowserPrint ${t} ${n} -> ${r}${o(i)}`);
}
async function c(e, t = {}) {
	let r = t.timeoutMs ?? 15e3, i = t.method ?? "GET", a = new AbortController(), o = setTimeout(() => a.abort(), r), c = !1;
	try {
		let n = await fetch(e, {
			method: i,
			body: t.body,
			headers: t.headers,
			signal: a.signal
		}), r = await n.text();
		if (t.raw) return s(t.debug, i, e, String(n.status), t.body), r;
		if (n.status !== 200) throw s(t.debug, i, e, String(n.status), t.body), c = !0, Error(r || `HTTP ${n.status}`);
		return s(t.debug, i, e, String(n.status), t.body), r;
	} catch (r) {
		throw c || s(t.debug, i, e, n(r), t.body), Error(n(r));
	} finally {
		clearTimeout(o);
	}
}
async function l(e, t = {}) {
	let r = t.timeoutMs ?? 15e3, i = new AbortController(), a = setTimeout(() => i.abort(), r), o = !1;
	try {
		let n = await fetch(e, {
			method: "GET",
			signal: i.signal
		});
		if (n.status !== 200) {
			let r = await n.text().catch(() => "");
			throw s(t.debug, "GET", e, String(n.status)), o = !0, Error(r || `HTTP ${n.status}`);
		}
		let r = await n.blob();
		return s(t.debug, "GET", e, `${n.status} body=${r.size} bytes`), r;
	} catch (r) {
		throw o || s(t.debug, "GET", e, n(r)), Error(n(r));
	} finally {
		clearTimeout(a);
	}
}
function u(e) {
	try {
		return JSON.parse(e);
	} catch {
		throw Error("Invalid JSON response from Browser Print agent");
	}
}
function d(e, t) {
	if (t == null || t.length === 0) return;
	let n;
	try {
		n = new URL(e);
	} catch {
		throw Error("Invalid URL");
	}
	if (!t.includes(n.origin)) throw Error(`URL origin not allowed: ${n.origin}`);
}
function f(e, t) {
	try {
		e?.(t);
	} catch {}
}
function p(e, t, r) {
	let i = n(r), a = e ?? t;
	try {
		a ? a(i) : console.error("BrowserPrint error (no errorCallback):", i);
	} catch {}
}
//#endregion
//#region \0@oxc-project+runtime@0.152.0/helpers/esm/typeof.js
function m(e) {
	"@babel/helpers - typeof";
	return m = typeof Symbol == "function" && typeof Symbol.iterator == "symbol" ? function(e) {
		return typeof e;
	} : function(e) {
		return e && typeof Symbol == "function" && e.constructor === Symbol && e !== Symbol.prototype ? "symbol" : typeof e;
	}, m(e);
}
//#endregion
//#region \0@oxc-project+runtime@0.152.0/helpers/esm/toPrimitive.js
function h(e, t) {
	if (m(e) != "object" || !e) return e;
	var n = e[Symbol.toPrimitive];
	if (n !== void 0) {
		var r = n.call(e, t || "default");
		if (m(r) != "object") return r;
		throw TypeError("@@toPrimitive must return a primitive value.");
	}
	return (t === "string" ? String : Number)(e);
}
//#endregion
//#region \0@oxc-project+runtime@0.152.0/helpers/esm/toPropertyKey.js
function g(e) {
	var t = h(e, "string");
	return m(t) == "symbol" ? t : t + "";
}
//#endregion
//#region \0@oxc-project+runtime@0.152.0/helpers/esm/defineProperty.js
function _(e, t, n) {
	return (t = g(t)) in e ? Object.defineProperty(e, t, {
		value: n,
		enumerable: !0,
		configurable: !0,
		writable: !0
	}) : e[t] = n, e;
}
var v = new TextEncoder();
function y(e) {
	return e.uid && e.uid.length > 0 ? e.uid : `${e.name ?? ""}|${e.connection ?? ""}|${e.deviceType ?? ""}`;
}
function b(e) {
	if (typeof e != "object" || !e || Array.isArray(e)) throw Error("Invalid device info from Browser Print agent");
	let t = e;
	for (let e of [
		"name",
		"deviceType",
		"connection",
		"uid",
		"provider",
		"manufacturer"
	]) {
		let n = t[e];
		if (n !== void 0 && typeof n != "string") throw Error(`Invalid device info field: ${e}`);
	}
	if (t.version !== void 0 && typeof t.version != "number") throw Error("Invalid device info field: version");
	return {
		name: t.name,
		deviceType: t.deviceType,
		connection: t.connection,
		uid: t.uid,
		version: t.version,
		provider: t.provider,
		manufacturer: t.manufacturer
	};
}
function x(e) {
	return {
		name: e.name,
		uid: e.uid,
		connection: e.connection,
		deviceType: e.deviceType,
		version: e.version,
		provider: e.provider,
		manufacturer: e.manufacturer
	};
}
function S(e, t) {
	return class {
		constructor(e) {
			_(this, "name", void 0), _(this, "deviceType", void 0), _(this, "connection", void 0), _(this, "uid", void 0), _(this, "version", void 0), _(this, "provider", void 0), _(this, "manufacturer", void 0), _(this, "readRetries", void 0), _(this, "sendErrorCallback", () => {}), _(this, "sendFinishedCallback", () => {}), _(this, "readErrorCallback", () => {}), _(this, "readFinishedCallback", () => {}), this.name = e.name, this.deviceType = e.deviceType, this.connection = e.connection, this.uid = e.uid, this.version = e.version ?? 2, this.provider = e.provider, this.manufacturer = e.manufacturer, this.readRetries = +(this.connection === "bluetooth");
		}
		send(n, i, a) {
			let o = i ?? this.sendFinishedCallback, s = a ?? this.sendErrorCallback;
			c(`${t}write`, r(e.debug, {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					device: x(this),
					data: n
				})
			})).then((e) => f(o, e)).catch((t) => p(s, e.defaultErrorCallback, t));
		}
		sendUrl(n, i, a, o) {
			let s = i ?? this.sendFinishedCallback, l = a ?? this.sendErrorCallback;
			try {
				d(n, e.allowedUrlOrigins);
			} catch (t) {
				p(l, e.defaultErrorCallback, t);
				return;
			}
			let u = {
				device: x(this),
				url: n
			};
			o != null && (u.options = o), c(`${t}write`, r(e.debug, {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify(u)
			})).then((e) => f(s, e)).catch((t) => p(l, e.defaultErrorCallback, t));
		}
		sendFile(n, i, a) {
			if (typeof n == "string") {
				e.loadFileFromUrl(n, (e) => this.sendFile(e, i, a), a);
				return;
			}
			let o = i ?? e.defaultSuccessCallback, s = a ?? e.defaultErrorCallback, l = new FormData();
			l.append("json", JSON.stringify({ device: x(this) })), l.append("blob", n), c(`${t}write`, r(e.debug, {
				method: "POST",
				body: l
			})).then((e) => f(o, e)).catch((t) => p(s, e.defaultErrorCallback, t));
		}
		convertAndSendFile(t, n, r, i) {
			let a = { ...i ?? {} };
			a.action || (a.action = "print"), e.convert(t, this, a, n, r);
		}
		read(n, i) {
			let a = n ?? this.readFinishedCallback, o = i ?? this.readErrorCallback;
			c(`${t}read`, r(e.debug, {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ device: x(this) })
			})).then((e) => f(a, e)).catch((t) => p(o, e.defaultErrorCallback, t));
		}
		readUntilStringReceived(t, n, r, i, a = "") {
			let o = i ?? this.readRetries, s = n ?? this.readFinishedCallback, c = r ?? this.readErrorCallback;
			this.read((n) => {
				let r = o;
				if (n && n.length !== 0) r = 0;
				else if (r <= 0) {
					f(s, a);
					return;
				}
				let i = a + n;
				if (v.encode(i).byteLength > 2097152) {
					p(c, e.defaultErrorCallback, "Read buffer exceeded maximum size");
					return;
				}
				t !== "" && i.includes(t) ? f(s, i) : this.readUntilStringReceived(t, s, c, r - 1, i);
			}, c);
		}
		readAllAvailable(e, t, n) {
			this.readUntilStringReceived("", e, t, n);
		}
		sendThenRead(e, t, n) {
			this.send(e, () => {
				this.read(t, n);
			}, n);
		}
		sendThenReadUntilStringReceived(e, t, n, r, i) {
			this.send(e, () => {
				this.readUntilStringReceived(t, n, r, i);
			}, r);
		}
		sendThenReadAllAvailable(e, t, n, r) {
			this.send(e, () => {
				this.readUntilStringReceived("", t, n, r);
			}, n);
		}
	};
}
//#endregion
//#region src/api.ts
function C(e) {
	return e.length < 3 ? "" : e.substring(e.length - 3);
}
function w(e) {
	return e.toLowerCase().replace("image/", "").replace("application/", "").replace("x-ms-", "");
}
function T(e = {}) {
	let n = typeof e == "string" ? { baseUrl: e } : e, i = n.baseUrl ?? t(), a = {};
	a.debug = n.debug ?? !1, a.allowedUrlOrigins = n.allowedUrlOrigins ?? null;
	let o = /* @__PURE__ */ new Map();
	return a.defaultSuccessCallback = () => {}, a.defaultErrorCallback = () => {}, a.ApplicationConfiguration = class {
		constructor() {
			_(this, "application", {
				version: "1.2.0.3",
				build_number: 3,
				api_level: 2,
				platform: "",
				supportedConversions: {}
			});
		}
	}, a.Device = S(a, i), a.getLocalDevices = (e, t, n) => {
		c(`${i}available`, r(a.debug, { method: "GET" })).then((t) => {
			let r = u(t);
			for (let e of Object.keys(r)) {
				let t = r[e];
				Array.isArray(t) && (r[e] = t.map((e) => new a.Device(b(e))));
			}
			n === void 0 ? f(e, r) : f(e, Array.isArray(r[n]) ? r[n] : []);
		}).catch((e) => p(t, a.defaultErrorCallback, e));
	}, a.getDefaultDevice = (e, t, n) => {
		let o = "default";
		e != null && (o = `default?type=${encodeURIComponent(e)}`), c(`${i}${o}`, r(a.debug, { method: "GET" })).then((e) => {
			if (e === "") {
				f(t, null);
				return;
			}
			let n = b(u(e));
			f(t, new a.Device(n));
		}).catch((e) => p(n, a.defaultErrorCallback, e));
	}, a.getApplicationConfiguration = (e, t) => {
		c(`${i}config`, r(a.debug, { method: "GET" })).then((t) => {
			if (t === "") {
				f(e, null);
				return;
			}
			f(e, u(t));
		}).catch((e) => p(t, a.defaultErrorCallback, e));
	}, a.readOnInterval = (e, t, n) => {
		let r = n;
		(r === void 0 || r === 0) && (r = 1);
		let i = y(e);
		a.stopReadOnInterval(e);
		let s = { stopped: !1 }, c = () => {
			s.stopped || e.read((e) => {
				s.stopped || o.get(i) !== s || (f(t, e), s.timer = setTimeout(c, r));
			}, () => {
				s.stopped || o.get(i) !== s || (s.timer = setTimeout(c, r));
			});
		};
		o.set(i, s), s.timer = setTimeout(c, r);
	}, a.stopReadOnInterval = (e) => {
		let t = y(e), n = o.get(t);
		n !== void 0 && (n.stopped = !0, n.timer !== void 0 && clearTimeout(n.timer), o.delete(t));
	}, a.bindFieldToReadData = (e, t, n, r) => {
		a.readOnInterval(e, (e) => {
			e !== "" && (t.value = e, r?.());
		}, n);
	}, a.loadFileFromUrl = (e, t, n) => {
		try {
			d(e, a.allowedUrlOrigins);
		} catch (e) {
			p(n, a.defaultErrorCallback, e);
			return;
		}
		l(e, { debug: a.debug }).then((e) => {
			t && f(t, e);
		}).catch((e) => p(n, a.defaultErrorCallback, e));
	}, a.convert = (e, t, n, o, s) => {
		if (!e) {
			p(s, a.defaultErrorCallback, "Resource not specified");
			return;
		}
		if (typeof e == "string") {
			let r = { ...n ?? {} };
			a.loadFileFromUrl(e, (n) => {
				r.fromFormat || (r.fromFormat = C(e)), a.convert(n, t, r, o, s);
			}, s);
			return;
		}
		let l = { ...n ?? {} };
		e.type && (e.type.startsWith("image/") || e.type.startsWith("application/")) && (l.fromFormat = w(e.type));
		let d = {};
		l != null && (d.options = l), t && (d.device = x(t));
		let m = new FormData();
		m.append("json", JSON.stringify(d)), m.append("blob", e), c(`${i}convert`, r(a.debug, {
			method: "POST",
			body: m
		})).then((e) => {
			o && f(o, u(e));
		}).catch((e) => p(s, a.defaultErrorCallback, e));
	}, a.scanImage = (e, t, n, o) => {
		if (!e) {
			p(o, a.defaultErrorCallback, "Resource not specified");
			return;
		}
		if (typeof e == "string") {
			let r = { ...t ?? {} };
			a.loadFileFromUrl(e, (t) => {
				r.format || (r.format = C(e)), a.scanImage(t, r, n, o);
			}, o);
			return;
		}
		let s = { ...t ?? {} };
		e.type && (e.type.startsWith("image/") || e.type.startsWith("application/")) && (s.format = w(e.type));
		let l = new FormData();
		l.append("json", JSON.stringify({ options: s })), l.append("blob", e), c(`${i}convert/scan`, r(a.debug, {
			method: "POST",
			body: l
		})).then((e) => {
			n && f(n, u(e));
		}).catch((e) => p(o, a.defaultErrorCallback, e));
	}, a;
}
//#endregion
//#region src/zebra/helpers.ts
function E(e, t) {
	if (/["\x00-\x1f\x7f]/.test(e)) throw Error(`Invalid SGD ${t}`);
}
function D(e) {
	return typeof e == "string" ? e : e instanceof Error ? e.message : String(e);
}
function O(e, t, n, r) {
	if (!t && !n) return new Promise((t, n) => e(t, n));
	e((e) => f(t, e), (e) => p(n, r, e));
}
function k(e) {
	return e.length > 1 && e.charAt(0) === "" && e.charAt(e.length - 1) === "";
}
function A(e) {
	return e.split("").map((e) => e.replace(/^[\s\x02]+/, "").trim()).filter((e) => e.length > 0).map((e) => e.split(",").map((e) => e.trim()));
}
function j(e, t) {
	return e?.[t] === "1";
}
function M(e, t) {
	let n = parseInt(e?.[t] ?? "", 10);
	return Number.isNaN(n) ? void 0 : n;
}
var N = class {
	constructor(e) {
		_(this, "raw", void 0), _(this, "offline", !1), _(this, "paperOut", !1), _(this, "paused", !1), _(this, "headOpen", !1), _(this, "ribbonOut", !1), _(this, "labelLengthDots", void 0), _(this, "formatsInBuffer", void 0), _(this, "bufferFull", !1), _(this, "partialFormatInProgress", !1), _(this, "corruptRam", !1), _(this, "underTemperature", !1), _(this, "overTemperature", !1), _(this, "labelsRemaining", void 0), this.raw = e ?? "";
		let t = this.raw.trim();
		if (!k(t)) {
			this.offline = !0;
			return;
		}
		let [n, r] = A(t);
		this.paperOut = j(n, 1), this.paused = j(n, 2), this.labelLengthDots = M(n, 3), this.formatsInBuffer = M(n, 4), this.bufferFull = j(n, 5), this.partialFormatInProgress = j(n, 7), this.corruptRam = j(n, 9), this.underTemperature = j(n, 10), this.overTemperature = j(n, 11), this.headOpen = j(r, 2), this.ribbonOut = j(r, 3), this.labelsRemaining = M(r, 8);
	}
	isFlagSet(e) {
		return this.raw.charAt(e) === "1";
	}
	isPrinterReady() {
		return !(this.paperOut || this.paused || this.headOpen || this.ribbonOut || this.offline);
	}
	getMessage() {
		return this.isPrinterReady() ? "Ready" : this.offline ? "Offline" : this.paperOut ? "Paper Out" : this.headOpen ? "Head Open" : this.ribbonOut ? "Ribbon Out" : this.paused ? "Paused" : "Ready";
	}
}, P = class {
	constructor(e) {
		if (_(this, "raw", void 0), _(this, "model", void 0), _(this, "firmware", void 0), _(this, "extra", void 0), !e) throw Error("Invalid Response");
		this.raw = e;
		let t = e.trim();
		if (!k(t)) throw Error("Invalid Response");
		let n = t.slice(1, -1).split(",");
		this.model = (n[0] ?? "").trim(), this.firmware = (n[1] ?? "").trim(), this.extra = n.slice(2).map((e) => e.trim());
	}
}, F = class {
	constructor(e) {
		if (_(this, "raw", void 0), _(this, "settings", {}), _(this, "darkness", void 0), _(this, "printSpeed", void 0), _(this, "printWidth", void 0), _(this, "labelLength", void 0), _(this, "firmwareVersion", void 0), _(this, "linkOSVersion", void 0), !e) throw Error("Invalid Response");
		let t = e.trim();
		if (this.raw = t, !k(t)) throw Error("Invalid Response");
		for (let e of t.replace("", "").replace("", "").split("\n")) {
			let t = e.trim();
			if (t === "") continue;
			let n = t.substring(0, 20).trim(), r = t.substring(20).trim();
			if (r === "") {
				let e = /^(.*?)\s{2,}(\S.*)$/.exec(t);
				if (!e) continue;
				n = e[1].trim(), r = e[2].trim();
			}
			this.settings[r] = n;
		}
		let n = this.settings;
		this.darkness = parseFloat(n.DARKNESS), this.printSpeed = parseInt((n["PRINT SPEED"] ?? "").replace("IPS", "").trim(), 10), this.printWidth = parseInt(n["PRINT WIDTH"], 10), this.labelLength = parseInt(n["LABEL LENGTH"], 10), this.firmwareVersion = (n.FIRMWARE ?? "").replace("<-", "").trim(), this.linkOSVersion = Object.prototype.hasOwnProperty.call(n, "LINK-OS VERSION") ? n["LINK-OS VERSION"] : "0";
	}
}, I = 5, L = 1e3;
function R(e) {
	let t = e.Device;
	class n extends t {
		constructor(e, t = {}) {
			super(e), _(this, "configuration", void 0), _(this, "queue", []), t.autoLoadConfiguration !== !1 && this.loadConfigurationInBackground(1);
		}
		loadConfigurationInBackground(e) {
			this.configuration || this.getConfiguration().catch(() => {
				e >= I || setTimeout(() => this.loadConfigurationInBackground(e + 1), L * 2 ** (e - 1)).unref?.();
			});
		}
		clearRequestQueue() {
			let e = this.queue[0]?.started ? this.queue[0] : void 0;
			for (let t of this.queue) t !== e && t.waiters.forEach((e) => e.reject("Request cancelled"));
			this.queue = e ? [e] : [];
		}
		enqueue(e, t, n) {
			return new Promise((r, i) => {
				let a = {
					resolve: r,
					reject: i
				}, o = e === "status" ? this.queue.find((e) => e.kind === "status" && !e.started) : void 0;
				if (o) {
					o.waiters.push(a);
					return;
				}
				this.queue.push({
					kind: e,
					command: t,
					parse: n,
					waiters: [a],
					started: !1
				}), this.pump();
			});
		}
		pump() {
			let e = this.queue[0];
			if (!e || e.started) return;
			e.started = !0;
			let t = !1, n = (n) => {
				if (t) return;
				t = !0;
				let r = this.queue.indexOf(e);
				r >= 0 && this.queue.splice(r, 1);
				for (let t of e.waiters) "error" in n ? t.reject(n.error) : t.resolve(n.value);
				this.pump();
			}, r = (t) => {
				try {
					n({ value: e.parse ? e.parse(t) : t });
				} catch (e) {
					n({ error: D(e) });
				}
			}, i = (e) => n({ error: e });
			e.kind === "set" ? this.send(e.command, r, i) : e.kind === "status" || e.kind === "info" || e.kind === "config" ? this.sendThenReadUntilStringReceived(e.command, "", r, i) : this.sendThenReadAllAvailable(e.command, r, i);
		}
		getStatus(t, n) {
			return O((e, t) => {
				this.enqueue("status", "~hs\r\n", (e) => new N(e)).then((t) => e(t), t);
			}, t, n, e.defaultErrorCallback);
		}
		isPrinterReady(t, n) {
			return O((e, t) => {
				this.getStatus().then((n) => n.isPrinterReady() ? e(n.getMessage()) : t(n.getMessage()), t);
			}, t, n, e.defaultErrorCallback);
		}
		getInfo(t, n) {
			return O((e, t) => {
				this.enqueue("info", "~hi\r\n", (e) => new P(e)).then((t) => e(t), t);
			}, t, n, e.defaultErrorCallback);
		}
		getConfiguration(t, n) {
			return O((e, t) => {
				this.enqueue("config", "^XA^HH^XZ", (e) => {
					let t = new F(e);
					return this.configuration = t, t;
				}).then((t) => e(t), t);
			}, t, n, e.defaultErrorCallback);
		}
		getSGD(t, n, r) {
			return O((e, n) => {
				try {
					E(t, "name");
				} catch (e) {
					n(D(e));
					return;
				}
				this.enqueue("sgd", `! U1 getvar "${t}"\r\n`).then((t) => e(t), n);
			}, n, r, e.defaultErrorCallback);
		}
		setSGD(t, n, r, i) {
			return O((e, r) => {
				try {
					E(t, "name"), E(n, "value");
				} catch (e) {
					r(D(e));
					return;
				}
				this.enqueue("set", `! U1 setvar "${t}" "${n}"\r\n`).then((t) => e(t), r);
			}, r, i, e.defaultErrorCallback);
		}
		setThenGetSGD(t, n, r, i) {
			return O((e, r) => {
				this.setSGD(t, n).then(() => this.getSGD(t).then(e, r), r);
			}, r, i, e.defaultErrorCallback);
		}
		query(t, n, r) {
			return O((e, n) => {
				this.enqueue("query", t).then((t) => e(t), n);
			}, n, r, e.defaultErrorCallback);
		}
		async ensureConfiguration() {
			return this.configuration ?? await this.getConfiguration();
		}
		convertWith(t, n, r, i, a) {
			return O((i, a) => {
				this.ensureConfiguration().then((o) => {
					let s = {
						...r ?? {},
						action: t
					};
					if (t === "print") {
						if (!Number.isFinite(o.printWidth) || o.printWidth <= 0 || !Number.isFinite(o.labelLength) || o.labelLength <= 0) {
							a("Printer configuration incomplete: printWidth/labelLength");
							return;
						}
						s.fitTo = {
							width: o.printWidth,
							height: o.labelLength
						};
					}
					e.convert(n, this, s, i, (e) => a(e || "Conversion is not supported by this Browser Print agent"));
				}, a);
			}, i, a, e.defaultErrorCallback);
		}
		printImageAsLabel(e, t, n, r) {
			return this.convertWith("print", e, t, n, r);
		}
		getConvertedResource(e, t, n, r) {
			return this.convertWith("return", e, t, n, r);
		}
		storeConvertedResource(e, t, n, r) {
			return this.convertWith("store", e, t, n, r);
		}
	}
	return _(n, "Status", N), _(n, "Info", P), _(n, "Configuration", F), n;
}
//#endregion
//#region src/zebra/watch.ts
function z(e, t) {
	let n = /* @__PURE__ */ new Map(), r;
	function i(e) {
		return y(e);
	}
	function a(e, t, r) {
		if (n.get(t) !== e) return;
		if (r.offline) {
			if (e.errors += 1, e.errors < e.errorsForOffline) return;
		} else e.errors = 0;
		let i = e.previous;
		if (e.previous = r, i === "" || i.raw !== r.raw || i.offline !== r.offline) try {
			e.onchange(i, r);
		} catch {}
	}
	function o() {
		for (let [e, t] of n) t.inFlight || (t.inFlight = !0, t.printer.getStatus().then((n) => {
			t.inFlight = !1, a(t, e, n);
		}, () => {
			t.inFlight = !1, a(t, e, new N(""));
		}));
	}
	return {
		watch(a, s, c = 2) {
			let l = a instanceof e ? a : new e(a, { autoLoadConfiguration: !1 });
			n.set(i(a), {
				printer: l,
				previous: "",
				onchange: s,
				errors: 0,
				errorsForOffline: c,
				inFlight: !1
			}), r === void 0 && (r = setInterval(o, t), r.unref?.());
		},
		stopWatching(e) {
			n.delete(i(e)), n.size === 0 && r !== void 0 && (clearInterval(r), r = void 0);
		}
	};
}
//#endregion
//#region src/zebra/create.ts
function B(e, t = {}) {
	let n = t.pollIntervalMs ?? 2e3, r = R(e), { watch: i, stopWatching: a } = z(r, n);
	return {
		Printer: r,
		watch: i,
		stopWatching: a
	};
}
//#endregion
//#region src/index.ts
var V = T(), H = B(V);
if (Object.assign(V, {
	createBrowserPrint: T,
	DEFAULT_TIMEOUT_MS: e,
	resolveBaseUrl: t,
	Zebra: H
}), typeof document < "u") {
	let e = globalThis;
	e.BrowserPrint ?? (e.BrowserPrint = V), e.Zebra ?? (e.Zebra = H);
}
//#endregion
export { V as default };

//# sourceMappingURL=browserprint.js.map