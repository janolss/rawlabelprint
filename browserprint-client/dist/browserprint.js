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
	try {
		e?.(t);
	} catch {}
}
function f(e, t, r) {
	let i = n(r), a = e ?? t;
	try {
		a ? a(i) : console.error("BrowserPrint error (no errorCallback):", i);
	} catch {}
}
//#endregion
//#region \0@oxc-project+runtime@0.152.0/helpers/esm/typeof.js
function p(e) {
	"@babel/helpers - typeof";
	return p = typeof Symbol == "function" && typeof Symbol.iterator == "symbol" ? function(e) {
		return typeof e;
	} : function(e) {
		return e && typeof Symbol == "function" && e.constructor === Symbol && e !== Symbol.prototype ? "symbol" : typeof e;
	}, p(e);
}
//#endregion
//#region \0@oxc-project+runtime@0.152.0/helpers/esm/toPrimitive.js
function m(e, t) {
	if (p(e) != "object" || !e) return e;
	var n = e[Symbol.toPrimitive];
	if (n !== void 0) {
		var r = n.call(e, t || "default");
		if (p(r) != "object") return r;
		throw TypeError("@@toPrimitive must return a primitive value.");
	}
	return (t === "string" ? String : Number)(e);
}
//#endregion
//#region \0@oxc-project+runtime@0.152.0/helpers/esm/toPropertyKey.js
function h(e) {
	var t = m(e, "string");
	return p(t) == "symbol" ? t : t + "";
}
//#endregion
//#region \0@oxc-project+runtime@0.152.0/helpers/esm/defineProperty.js
function g(e, t, n) {
	return (t = h(t)) in e ? Object.defineProperty(e, t, {
		value: n,
		enumerable: !0,
		configurable: !0,
		writable: !0
	}) : e[t] = n, e;
}
//#endregion
//#region src/device.ts
function _(e) {
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
function v(e, t) {
	return class {
		constructor(e) {
			g(this, "name", void 0), g(this, "deviceType", void 0), g(this, "connection", void 0), g(this, "uid", void 0), g(this, "version", void 0), g(this, "provider", void 0), g(this, "manufacturer", void 0), g(this, "readRetries", void 0), g(this, "sendErrorCallback", () => {}), g(this, "sendFinishedCallback", () => {}), g(this, "readErrorCallback", () => {}), g(this, "readFinishedCallback", () => {}), this.name = e.name, this.deviceType = e.deviceType, this.connection = e.connection, this.uid = e.uid, this.version = e.version ?? 2, this.provider = e.provider, this.manufacturer = e.manufacturer, this.readRetries = +(this.connection === "bluetooth");
		}
		send(n, i, a) {
			let o = i ?? this.sendFinishedCallback, s = a ?? this.sendErrorCallback;
			c(`${t}write`, r(e.debug, {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					device: _(this),
					data: n
				})
			})).then((e) => d(o, e)).catch((t) => f(s, e.defaultErrorCallback, t));
		}
		sendUrl(n, i, a, o) {
			let s = i ?? this.sendFinishedCallback, l = a ?? this.sendErrorCallback, u = {
				device: _(this),
				url: n
			};
			o != null && (u.options = o), c(`${t}write`, r(e.debug, {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify(u)
			})).then((e) => d(s, e)).catch((t) => f(l, e.defaultErrorCallback, t));
		}
		sendFile(n, i, a) {
			if (typeof n == "string") {
				e.loadFileFromUrl(n, (e) => this.sendFile(e, i, a), a);
				return;
			}
			let o = i ?? e.defaultSuccessCallback, s = a ?? e.defaultErrorCallback, l = new FormData();
			l.append("json", JSON.stringify({ device: _(this) })), l.append("blob", n), c(`${t}write`, r(e.debug, {
				method: "POST",
				body: l
			})).then((e) => d(o, e)).catch((t) => f(s, e.defaultErrorCallback, t));
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
				body: JSON.stringify({ device: _(this) })
			})).then((e) => d(a, e)).catch((t) => f(o, e.defaultErrorCallback, t));
		}
		readUntilStringReceived(e, t, n, r, i = "") {
			let a = r ?? this.readRetries, o = t ?? this.readFinishedCallback, s = n ?? this.readErrorCallback;
			this.read((t) => {
				let n = a;
				if (t && t.length !== 0) n = 0;
				else if (n <= 0) {
					d(o, i);
					return;
				}
				let r = i + t;
				e !== "" && r.includes(e) ? d(o, r) : this.readUntilStringReceived(e, o, s, n - 1, r);
			}, s);
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
function y(e) {
	return e.uid && e.uid.length > 0 ? e.uid : `${e.name ?? ""}|${e.connection ?? ""}|${e.deviceType ?? ""}`;
}
function b(e) {
	return e.length < 3 ? "" : e.substring(e.length - 3);
}
function x(e) {
	return e.toLowerCase().replace("image/", "").replace("application/", "").replace("x-ms-", "");
}
function S(e = {}) {
	let n = typeof e == "string" ? { baseUrl: e } : e, i = n.baseUrl ?? t(), a = {};
	a.debug = n.debug ?? !1;
	let o = /* @__PURE__ */ new Map();
	return a.defaultSuccessCallback = () => {}, a.defaultErrorCallback = () => {}, a.ApplicationConfiguration = class {
		constructor() {
			g(this, "application", {
				version: "1.2.0.3",
				build_number: 3,
				api_level: 2,
				platform: "",
				supportedConversions: {}
			});
		}
	}, a.Device = v(a, i), a.getLocalDevices = (e, t, n) => {
		c(`${i}available`, r(a.debug, { method: "GET" })).then((t) => {
			let r = u(t);
			for (let e of Object.keys(r)) {
				let t = r[e];
				Array.isArray(t) && (r[e] = t.map((e) => new a.Device(e)));
			}
			n === void 0 ? d(e, r) : d(e, Array.isArray(r[n]) ? r[n] : []);
		}).catch((e) => f(t, a.defaultErrorCallback, e));
	}, a.getDefaultDevice = (e, t, n) => {
		let o = "default";
		e != null && (o = `default?type=${encodeURIComponent(e)}`), c(`${i}${o}`, r(a.debug, { method: "GET" })).then((e) => {
			if (e === "") {
				d(t, null);
				return;
			}
			let n = u(e);
			d(t, new a.Device(n));
		}).catch((e) => f(n, a.defaultErrorCallback, e));
	}, a.getApplicationConfiguration = (e, t) => {
		c(`${i}config`, r(a.debug, { method: "GET" })).then((t) => {
			if (t === "") {
				d(e, null);
				return;
			}
			d(e, u(t));
		}).catch((e) => f(t, a.defaultErrorCallback, e));
	}, a.readOnInterval = (e, t, n) => {
		let r = n;
		(r === void 0 || r === 0) && (r = 1);
		let i = y(e);
		a.stopReadOnInterval(e);
		let s = { stopped: !1 }, c = () => {
			s.stopped || e.read((e) => {
				s.stopped || o.get(i) !== s || (d(t, e), s.timer = setTimeout(c, r));
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
		l(e, { debug: a.debug }).then((e) => {
			t && d(t, e);
		}).catch((e) => f(n, a.defaultErrorCallback, e));
	}, a.convert = (e, t, n, o, s) => {
		if (!e) {
			f(s, a.defaultErrorCallback, "Resource not specified");
			return;
		}
		if (typeof e == "string") {
			let r = { ...n ?? {} };
			a.loadFileFromUrl(e, (n) => {
				r.fromFormat || (r.fromFormat = b(e)), a.convert(n, t, r, o, s);
			}, s);
			return;
		}
		let l = { ...n ?? {} };
		e.type && (e.type.startsWith("image/") || e.type.startsWith("application/")) && (l.fromFormat = x(e.type));
		let p = {};
		l != null && (p.options = l), t && (p.device = _(t));
		let m = new FormData();
		m.append("json", JSON.stringify(p)), m.append("blob", e), c(`${i}convert`, r(a.debug, {
			method: "POST",
			body: m
		})).then((e) => {
			o && d(o, u(e));
		}).catch((e) => f(s, a.defaultErrorCallback, e));
	}, a.scanImage = (e, t, n, o) => {
		if (!e) {
			f(o, a.defaultErrorCallback, "Resource not specified");
			return;
		}
		if (typeof e == "string") {
			let r = { ...t ?? {} };
			a.loadFileFromUrl(e, (t) => {
				r.format || (r.format = b(e)), a.scanImage(t, r, n, o);
			}, o);
			return;
		}
		let s = { ...t ?? {} };
		e.type && (e.type.startsWith("image/") || e.type.startsWith("application/")) && (s.format = x(e.type));
		let l = new FormData();
		l.append("json", JSON.stringify({ options: s })), l.append("blob", e), c(`${i}convert/scan`, r(a.debug, {
			method: "POST",
			body: l
		})).then((e) => {
			n && d(n, u(e));
		}).catch((e) => f(o, a.defaultErrorCallback, e));
	}, a;
}
function C(e) {
	return e.length > 1 && e.charAt(0) === "" && e.charAt(e.length - 1) === "";
}
function w(e) {
	return typeof e == "string" ? e : e instanceof Error ? e.message : String(e);
}
function T(e) {
	return e.split("").map((e) => e.replace(/^[\s\x02]+/, "").trim()).filter((e) => e.length > 0).map((e) => e.split(",").map((e) => e.trim()));
}
function E(e, t) {
	return e?.[t] === "1";
}
function D(e, t) {
	let n = parseInt(e?.[t] ?? "", 10);
	return Number.isNaN(n) ? void 0 : n;
}
var O = class {
	constructor(e) {
		g(this, "raw", void 0), g(this, "offline", !1), g(this, "paperOut", !1), g(this, "paused", !1), g(this, "headOpen", !1), g(this, "ribbonOut", !1), g(this, "labelLengthDots", void 0), g(this, "formatsInBuffer", void 0), g(this, "bufferFull", !1), g(this, "partialFormatInProgress", !1), g(this, "corruptRam", !1), g(this, "underTemperature", !1), g(this, "overTemperature", !1), g(this, "labelsRemaining", void 0), this.raw = e ?? "";
		let t = this.raw.trim();
		if (!C(t)) {
			this.offline = !0;
			return;
		}
		let [n, r] = T(t);
		this.paperOut = E(n, 1), this.paused = E(n, 2), this.labelLengthDots = D(n, 3), this.formatsInBuffer = D(n, 4), this.bufferFull = E(n, 5), this.partialFormatInProgress = E(n, 7), this.corruptRam = E(n, 9), this.underTemperature = E(n, 10), this.overTemperature = E(n, 11), this.headOpen = E(r, 2), this.ribbonOut = E(r, 3), this.labelsRemaining = D(r, 8);
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
}, k = class {
	constructor(e) {
		if (g(this, "raw", void 0), g(this, "model", void 0), g(this, "firmware", void 0), g(this, "extra", void 0), !e) throw Error("Invalid Response");
		this.raw = e;
		let t = e.trim();
		if (!C(t)) throw Error("Invalid Response");
		let n = t.slice(1, -1).split(",");
		this.model = (n[0] ?? "").trim(), this.firmware = (n[1] ?? "").trim(), this.extra = n.slice(2).map((e) => e.trim());
	}
}, A = class {
	constructor(e) {
		if (g(this, "raw", void 0), g(this, "settings", {}), g(this, "darkness", void 0), g(this, "printSpeed", void 0), g(this, "printWidth", void 0), g(this, "labelLength", void 0), g(this, "firmwareVersion", void 0), g(this, "linkOSVersion", void 0), !e) throw Error("Invalid Response");
		let t = e.trim();
		if (this.raw = t, !C(t)) throw Error("Invalid Response");
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
}, j = 5, M = 1e3;
function N(e, t, n, r) {
	if (!t && !n) return new Promise((t, n) => e(t, n));
	e((e) => t?.(e), (e) => f(n, r, e));
}
function P(e, t = {}) {
	let n = t.pollIntervalMs ?? 2e3, r = e.Device;
	class i extends r {
		constructor(e, t = {}) {
			super(e), g(this, "configuration", void 0), g(this, "queue", []), t.autoLoadConfiguration !== !1 && this.loadConfigurationInBackground(1);
		}
		loadConfigurationInBackground(e) {
			this.configuration || this.getConfiguration().catch(() => {
				e >= j || setTimeout(() => this.loadConfigurationInBackground(e + 1), M * 2 ** (e - 1)).unref?.();
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
					n({ error: w(e) });
				}
			}, i = (e) => n({ error: e });
			e.kind === "set" ? this.send(e.command, r, i) : e.kind === "status" || e.kind === "info" || e.kind === "config" ? this.sendThenReadUntilStringReceived(e.command, "", r, i) : this.sendThenReadAllAvailable(e.command, r, i);
		}
		getStatus(t, n) {
			return N((e, t) => {
				this.enqueue("status", "~hs\r\n", (e) => new O(e)).then((t) => e(t), t);
			}, t, n, e.defaultErrorCallback);
		}
		isPrinterReady(t, n) {
			return N((e, t) => {
				this.getStatus().then((n) => n.isPrinterReady() ? e(n.getMessage()) : t(n.getMessage()), t);
			}, t, n, e.defaultErrorCallback);
		}
		getInfo(t, n) {
			return N((e, t) => {
				this.enqueue("info", "~hi\r\n", (e) => new k(e)).then((t) => e(t), t);
			}, t, n, e.defaultErrorCallback);
		}
		getConfiguration(t, n) {
			return N((e, t) => {
				this.enqueue("config", "^XA^HH^XZ", (e) => {
					let t = new A(e);
					return this.configuration = t, t;
				}).then((t) => e(t), t);
			}, t, n, e.defaultErrorCallback);
		}
		getSGD(t, n, r) {
			return N((e, n) => {
				this.enqueue("sgd", `! U1 getvar "${t}"\r\n`).then((t) => e(t), n);
			}, n, r, e.defaultErrorCallback);
		}
		setSGD(t, n, r, i) {
			return N((e, r) => {
				this.enqueue("set", `! U1 setvar "${t}" "${n}"\r\n`).then((t) => e(t), r);
			}, r, i, e.defaultErrorCallback);
		}
		setThenGetSGD(t, n, r, i) {
			return N((e, r) => {
				this.setSGD(t, n).then(() => this.getSGD(t).then(e, r), r);
			}, r, i, e.defaultErrorCallback);
		}
		query(t, n, r) {
			return N((e, n) => {
				this.enqueue("query", t).then((t) => e(t), n);
			}, n, r, e.defaultErrorCallback);
		}
		async ensureConfiguration() {
			return this.configuration ?? await this.getConfiguration();
		}
		convertWith(t, n, r, i, a) {
			return N((i, a) => {
				this.ensureConfiguration().then((o) => {
					let s = {
						...r ?? {},
						action: t
					};
					t === "print" && (s.fitTo = {
						width: o.printWidth,
						height: o.labelLength
					}), e.convert(n, this, s, i, (e) => a(e || "Conversion is not supported by this Browser Print agent"));
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
	g(i, "Status", O), g(i, "Info", k), g(i, "Configuration", A);
	let a = /* @__PURE__ */ new Map(), o;
	function s(e) {
		return e.uid ?? `${e.name ?? ""}|${e.connection ?? ""}`;
	}
	function c(e, t, n) {
		if (a.get(t) !== e) return;
		if (n.offline) {
			if (e.errors += 1, e.errors < e.errorsForOffline) return;
		} else e.errors = 0;
		let r = e.previous;
		if (e.previous = n, r === "" || r.raw !== n.raw || r.offline !== n.offline) try {
			e.onchange(r, n);
		} catch {}
	}
	function l() {
		for (let [e, t] of a) t.inFlight || (t.inFlight = !0, t.printer.getStatus().then((n) => {
			t.inFlight = !1, c(t, e, n);
		}, () => {
			t.inFlight = !1, c(t, e, new O(""));
		}));
	}
	return {
		Printer: i,
		watch(e, t, r = 2) {
			let c = e instanceof i ? e : new i(e, { autoLoadConfiguration: !1 });
			a.set(s(e), {
				printer: c,
				previous: "",
				onchange: t,
				errors: 0,
				errorsForOffline: r,
				inFlight: !1
			}), o === void 0 && (o = setInterval(l, n), o.unref?.());
		},
		stopWatching(e) {
			a.delete(s(e)), a.size === 0 && o !== void 0 && (clearInterval(o), o = void 0);
		}
	};
}
//#endregion
//#region src/index.ts
var F = S(), I = P(F);
if (Object.assign(F, {
	createBrowserPrint: S,
	DEFAULT_TIMEOUT_MS: e,
	resolveBaseUrl: t,
	Zebra: I
}), typeof globalThis < "u") {
	let e = globalThis;
	e.BrowserPrint = F, e.Zebra = I;
}
//#endregion
export { F as default };

//# sourceMappingURL=browserprint.js.map