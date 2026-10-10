//#region src/http.ts
var e = 15e3;
function t(e = globalThis.navigator?.userAgent, t = globalThis.location?.protocol) {
	let n = e ?? "";
	return /^((?!chrome|android).)*safari/i.test(n) && t === "https:" ? "https://127.0.0.1:9101/" : "http://127.0.0.1:9100/";
}
function n(e) {
	return e instanceof Error ? e.name === "AbortError" ? "Request timed out" : e.message || String(e) : String(e);
}
async function r(e, t = {}) {
	let r = t.timeoutMs ?? 15e3, i = new AbortController(), a = setTimeout(() => i.abort(), r);
	try {
		let n = await fetch(e, {
			method: t.method ?? "GET",
			body: t.body,
			headers: t.headers,
			signal: i.signal
		}), r = await n.text();
		if (t.raw) return r;
		if (n.status !== 200) throw Error(r || `HTTP ${n.status}`);
		return r;
	} catch (e) {
		throw Error(n(e));
	} finally {
		clearTimeout(a);
	}
}
async function i(e, t = {}) {
	let r = t.timeoutMs ?? 15e3, i = new AbortController(), a = setTimeout(() => i.abort(), r);
	try {
		let t = await fetch(e, {
			method: "GET",
			signal: i.signal
		});
		if (t.status !== 200) {
			let e = await t.text().catch(() => "");
			throw Error(e || `HTTP ${t.status}`);
		}
		return await t.blob();
	} catch (e) {
		throw Error(n(e));
	} finally {
		clearTimeout(a);
	}
}
function a(e) {
	try {
		return JSON.parse(e);
	} catch {
		throw Error("Invalid JSON response from Browser Print agent");
	}
}
function o(e, t) {
	try {
		e?.(t);
	} catch {}
}
function s(e, t, r) {
	let i = n(r), a = e ?? t;
	try {
		a ? a(i) : console.error("BrowserPrint error (no errorCallback):", i);
	} catch {}
}
//#endregion
//#region \0@oxc-project+runtime@0.153.0/helpers/esm/typeof.js
function c(e) {
	"@babel/helpers - typeof";
	return c = typeof Symbol == "function" && typeof Symbol.iterator == "symbol" ? function(e) {
		return typeof e;
	} : function(e) {
		return e && typeof Symbol == "function" && e.constructor === Symbol && e !== Symbol.prototype ? "symbol" : typeof e;
	}, c(e);
}
//#endregion
//#region \0@oxc-project+runtime@0.153.0/helpers/esm/toPrimitive.js
function l(e, t) {
	if (c(e) != "object" || !e) return e;
	var n = e[Symbol.toPrimitive];
	if (n !== void 0) {
		var r = n.call(e, t || "default");
		if (c(r) != "object") return r;
		throw TypeError("@@toPrimitive must return a primitive value.");
	}
	return (t === "string" ? String : Number)(e);
}
//#endregion
//#region \0@oxc-project+runtime@0.153.0/helpers/esm/toPropertyKey.js
function u(e) {
	var t = l(e, "string");
	return c(t) == "symbol" ? t : t + "";
}
//#endregion
//#region \0@oxc-project+runtime@0.153.0/helpers/esm/defineProperty.js
function d(e, t, n) {
	return (t = u(t)) in e ? Object.defineProperty(e, t, {
		value: n,
		enumerable: !0,
		configurable: !0,
		writable: !0
	}) : e[t] = n, e;
}
//#endregion
//#region src/device.ts
function f(e) {
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
function p(e, t) {
	return class {
		constructor(e) {
			d(this, "name", void 0), d(this, "deviceType", void 0), d(this, "connection", void 0), d(this, "uid", void 0), d(this, "version", void 0), d(this, "provider", void 0), d(this, "manufacturer", void 0), d(this, "readRetries", void 0), d(this, "sendErrorCallback", () => {}), d(this, "sendFinishedCallback", () => {}), d(this, "readErrorCallback", () => {}), d(this, "readFinishedCallback", () => {}), this.name = e.name, this.deviceType = e.deviceType, this.connection = e.connection, this.uid = e.uid, this.version = e.version ?? 2, this.provider = e.provider, this.manufacturer = e.manufacturer, this.readRetries = +(this.connection === "bluetooth");
		}
		send(n, i, a) {
			let c = i ?? this.sendFinishedCallback, l = a ?? this.sendErrorCallback;
			r(`${t}write`, {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					device: f(this),
					data: n
				})
			}).then((e) => o(c, e)).catch((t) => s(l, e.defaultErrorCallback, t));
		}
		sendUrl(n, i, a, c) {
			let l = i ?? this.sendFinishedCallback, u = a ?? this.sendErrorCallback, d = {
				device: f(this),
				url: n
			};
			c != null && (d.options = c), r(`${t}write`, {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify(d)
			}).then((e) => o(l, e)).catch((t) => s(u, e.defaultErrorCallback, t));
		}
		sendFile(n, i, a) {
			if (typeof n == "string") {
				e.loadFileFromUrl(n, (e) => this.sendFile(e, i, a), a);
				return;
			}
			let c = i ?? e.defaultSuccessCallback, l = a ?? e.defaultErrorCallback, u = new FormData();
			u.append("json", JSON.stringify({ device: f(this) })), u.append("blob", n), r(`${t}write`, {
				method: "POST",
				body: u
			}).then((e) => o(c, e)).catch((t) => s(l, e.defaultErrorCallback, t));
		}
		convertAndSendFile(t, n, r, i) {
			let a = { ...i ?? {} };
			a.action || (a.action = "print"), e.convert(t, this, a, n, r);
		}
		read(n, i) {
			let a = n ?? this.readFinishedCallback, c = i ?? this.readErrorCallback;
			r(`${t}read`, {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ device: f(this) })
			}).then((e) => o(a, e)).catch((t) => s(c, e.defaultErrorCallback, t));
		}
		readUntilStringReceived(e, t, n, r, i = "") {
			let a = r ?? this.readRetries, s = t ?? this.readFinishedCallback, c = n ?? this.readErrorCallback;
			this.read((t) => {
				let n = a;
				if (t && t.length !== 0) n = 0;
				else if (n <= 0) {
					o(s, i);
					return;
				}
				let r = i + t;
				e !== "" && r.includes(e) ? o(s, r) : this.readUntilStringReceived(e, s, c, n - 1, r);
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
function m(e) {
	return e.uid && e.uid.length > 0 ? e.uid : `${e.name ?? ""}|${e.connection ?? ""}|${e.deviceType ?? ""}`;
}
function h(e) {
	return e.length < 3 ? "" : e.substring(e.length - 3);
}
function g(e) {
	return e.toLowerCase().replace("image/", "").replace("application/", "").replace("x-ms-", "");
}
function _(e = t()) {
	let n = {}, c = /* @__PURE__ */ new Map();
	return n.defaultSuccessCallback = () => {}, n.defaultErrorCallback = () => {}, n.ApplicationConfiguration = class {
		constructor() {
			d(this, "application", {
				version: "1.2.0.3",
				build_number: 3,
				api_level: 2,
				platform: "",
				supportedConversions: {}
			});
		}
	}, n.Device = p(n, e), n.getLocalDevices = (t, i, c) => {
		r(`${e}available`, { method: "GET" }).then((e) => {
			let r = a(e);
			for (let e of Object.keys(r)) {
				let t = r[e];
				Array.isArray(t) && (r[e] = t.map((e) => new n.Device(e)));
			}
			c === void 0 ? o(t, r) : o(t, Array.isArray(r[c]) ? r[c] : []);
		}).catch((e) => s(i, n.defaultErrorCallback, e));
	}, n.getDefaultDevice = (t, i, c) => {
		let l = "default";
		t != null && (l = `default?type=${encodeURIComponent(t)}`), r(`${e}${l}`, { method: "GET" }).then((e) => {
			if (e === "") {
				o(i, null);
				return;
			}
			let t = a(e);
			o(i, new n.Device(t));
		}).catch((e) => s(c, n.defaultErrorCallback, e));
	}, n.getApplicationConfiguration = (t, i) => {
		r(`${e}config`, { method: "GET" }).then((e) => {
			e === "" ? o(t, null) : o(t, a(e));
		}).catch((e) => s(i, n.defaultErrorCallback, e));
	}, n.readOnInterval = (e, t, r) => {
		let i = r;
		(i === void 0 || i === 0) && (i = 1);
		let a = m(e);
		n.stopReadOnInterval(e);
		let s = { stopped: !1 }, l = () => {
			s.stopped || e.read((e) => {
				s.stopped || c.get(a) !== s || (o(t, e), s.timer = setTimeout(l, i));
			}, () => {
				s.stopped || c.get(a) !== s || (s.timer = setTimeout(l, i));
			});
		};
		c.set(a, s), s.timer = setTimeout(l, i);
	}, n.stopReadOnInterval = (e) => {
		let t = m(e), n = c.get(t);
		n !== void 0 && (n.stopped = !0, n.timer !== void 0 && clearTimeout(n.timer), c.delete(t));
	}, n.bindFieldToReadData = (e, t, r, i) => {
		n.readOnInterval(e, (e) => {
			e !== "" && (t.value = e, i?.());
		}, r);
	}, n.loadFileFromUrl = (e, t, r) => {
		i(e).then((e) => {
			t && o(t, e);
		}).catch((e) => s(r, n.defaultErrorCallback, e));
	}, n.convert = (t, i, c, l, u) => {
		if (!t) {
			s(u, n.defaultErrorCallback, "Resource not specified");
			return;
		}
		if (typeof t == "string") {
			let e = { ...c ?? {} };
			n.loadFileFromUrl(t, (r) => {
				e.fromFormat || (e.fromFormat = h(t)), n.convert(r, i, e, l, u);
			}, u);
			return;
		}
		let d = { ...c ?? {} };
		t.type && (t.type.startsWith("image/") || t.type.startsWith("application/")) && (d.fromFormat = g(t.type));
		let p = {};
		d != null && (p.options = d), i && (p.device = f(i));
		let m = new FormData();
		m.append("json", JSON.stringify(p)), m.append("blob", t), r(`${e}convert`, {
			method: "POST",
			body: m
		}).then((e) => {
			l && o(l, a(e));
		}).catch((e) => s(u, n.defaultErrorCallback, e));
	}, n.scanImage = (t, i, c, l) => {
		if (!t) {
			s(l, n.defaultErrorCallback, "Resource not specified");
			return;
		}
		if (typeof t == "string") {
			let e = { ...i ?? {} };
			n.loadFileFromUrl(t, (r) => {
				e.format || (e.format = h(t)), n.scanImage(r, e, c, l);
			}, l);
			return;
		}
		let u = { ...i ?? {} };
		t.type && (t.type.startsWith("image/") || t.type.startsWith("application/")) && (u.format = g(t.type));
		let d = new FormData();
		d.append("json", JSON.stringify({ options: u })), d.append("blob", t), r(`${e}convert/scan`, {
			method: "POST",
			body: d
		}).then((e) => {
			c && o(c, a(e));
		}).catch((e) => s(l, n.defaultErrorCallback, e));
	}, n;
}
function v(e) {
	return e.length > 1 && e.charAt(0) === "" && e.charAt(e.length - 1) === "";
}
function y(e) {
	return typeof e == "string" ? e : e instanceof Error ? e.message : String(e);
}
function b(e) {
	return e.split("").map((e) => e.replace(/^[\s\x02]+/, "").trim()).filter((e) => e.length > 0).map((e) => e.split(",").map((e) => e.trim()));
}
function x(e, t) {
	return e?.[t] === "1";
}
function S(e, t) {
	let n = parseInt(e?.[t] ?? "", 10);
	return Number.isNaN(n) ? void 0 : n;
}
var C = class {
	constructor(e) {
		d(this, "raw", void 0), d(this, "offline", !1), d(this, "paperOut", !1), d(this, "paused", !1), d(this, "headOpen", !1), d(this, "ribbonOut", !1), d(this, "labelLengthDots", void 0), d(this, "formatsInBuffer", void 0), d(this, "bufferFull", !1), d(this, "partialFormatInProgress", !1), d(this, "corruptRam", !1), d(this, "underTemperature", !1), d(this, "overTemperature", !1), d(this, "labelsRemaining", void 0), this.raw = e ?? "";
		let t = this.raw.trim();
		if (!v(t)) {
			this.offline = !0;
			return;
		}
		let [n, r] = b(t);
		this.paperOut = x(n, 1), this.paused = x(n, 2), this.labelLengthDots = S(n, 3), this.formatsInBuffer = S(n, 4), this.bufferFull = x(n, 5), this.partialFormatInProgress = x(n, 7), this.corruptRam = x(n, 9), this.underTemperature = x(n, 10), this.overTemperature = x(n, 11), this.headOpen = x(r, 2), this.ribbonOut = x(r, 3), this.labelsRemaining = S(r, 8);
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
}, w = class {
	constructor(e) {
		if (d(this, "raw", void 0), d(this, "model", void 0), d(this, "firmware", void 0), d(this, "extra", void 0), !e) throw Error("Invalid Response");
		this.raw = e;
		let t = e.trim();
		if (!v(t)) throw Error("Invalid Response");
		let n = t.slice(1, -1).split(",");
		this.model = (n[0] ?? "").trim(), this.firmware = (n[1] ?? "").trim(), this.extra = n.slice(2).map((e) => e.trim());
	}
}, T = class {
	constructor(e) {
		if (d(this, "raw", void 0), d(this, "settings", {}), d(this, "darkness", void 0), d(this, "printSpeed", void 0), d(this, "printWidth", void 0), d(this, "labelLength", void 0), d(this, "firmwareVersion", void 0), d(this, "linkOSVersion", void 0), !e) throw Error("Invalid Response");
		let t = e.trim();
		if (this.raw = t, !v(t)) throw Error("Invalid Response");
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
}, E = 5, D = 1e3;
function O(e, t, n, r) {
	if (!t && !n) return new Promise((t, n) => e(t, n));
	e((e) => t?.(e), (e) => s(n, r, e));
}
function k(e, t = {}) {
	let n = t.pollIntervalMs ?? 2e3, r = e.Device;
	class i extends r {
		constructor(e, t = {}) {
			super(e), d(this, "configuration", void 0), d(this, "queue", []), t.autoLoadConfiguration !== !1 && this.loadConfigurationInBackground(1);
		}
		loadConfigurationInBackground(e) {
			this.configuration || this.getConfiguration().catch(() => {
				e >= E || setTimeout(() => this.loadConfigurationInBackground(e + 1), D * 2 ** (e - 1)).unref?.();
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
				o ? o.waiters.push(a) : (this.queue.push({
					kind: e,
					command: t,
					parse: n,
					waiters: [a],
					started: !1
				}), this.pump());
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
					n({ error: y(e) });
				}
			}, i = (e) => n({ error: e });
			e.kind === "set" ? this.send(e.command, r, i) : e.kind === "status" || e.kind === "info" || e.kind === "config" ? this.sendThenReadUntilStringReceived(e.command, "", r, i) : this.sendThenReadAllAvailable(e.command, r, i);
		}
		getStatus(t, n) {
			return O((e, t) => {
				this.enqueue("status", "~hs\r\n", (e) => new C(e)).then((t) => e(t), t);
			}, t, n, e.defaultErrorCallback);
		}
		isPrinterReady(t, n) {
			return O((e, t) => {
				this.getStatus().then((n) => n.isPrinterReady() ? e(n.getMessage()) : t(n.getMessage()), t);
			}, t, n, e.defaultErrorCallback);
		}
		getInfo(t, n) {
			return O((e, t) => {
				this.enqueue("info", "~hi\r\n", (e) => new w(e)).then((t) => e(t), t);
			}, t, n, e.defaultErrorCallback);
		}
		getConfiguration(t, n) {
			return O((e, t) => {
				this.enqueue("config", "^XA^HH^XZ", (e) => {
					let t = new T(e);
					return this.configuration = t, t;
				}).then((t) => e(t), t);
			}, t, n, e.defaultErrorCallback);
		}
		getSGD(t, n, r) {
			return O((e, n) => {
				this.enqueue("sgd", `! U1 getvar "${t}"\r\n`).then((t) => e(t), n);
			}, n, r, e.defaultErrorCallback);
		}
		setSGD(t, n, r, i) {
			return O((e, r) => {
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
	d(i, "Status", C), d(i, "Info", w), d(i, "Configuration", T);
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
			t.inFlight = !1, c(t, e, new C(""));
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
var A = _(), j = k(A);
if (Object.assign(A, {
	createBrowserPrint: _,
	DEFAULT_TIMEOUT_MS: e,
	resolveBaseUrl: t,
	Zebra: j
}), typeof globalThis < "u") {
	let e = globalThis;
	e.BrowserPrint = A, e.Zebra = j;
}
//#endregion
export { A as default };

//# sourceMappingURL=browserprint.js.map