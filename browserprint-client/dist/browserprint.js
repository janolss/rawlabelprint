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
//#region \0@oxc-project+runtime@0.152.0/helpers/esm/typeof.js
function c(e) {
	"@babel/helpers - typeof";
	return c = typeof Symbol == "function" && typeof Symbol.iterator == "symbol" ? function(e) {
		return typeof e;
	} : function(e) {
		return e && typeof Symbol == "function" && e.constructor === Symbol && e !== Symbol.prototype ? "symbol" : typeof e;
	}, c(e);
}
//#endregion
//#region \0@oxc-project+runtime@0.152.0/helpers/esm/toPrimitive.js
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
//#region \0@oxc-project+runtime@0.152.0/helpers/esm/toPropertyKey.js
function u(e) {
	var t = l(e, "string");
	return c(t) == "symbol" ? t : t + "";
}
//#endregion
//#region \0@oxc-project+runtime@0.152.0/helpers/esm/defineProperty.js
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
			if (e === "") {
				o(t, null);
				return;
			}
			o(t, a(e));
		}).catch((e) => s(i, n.defaultErrorCallback, e));
	}, n.readOnInterval = (e, t, n) => {
		let r = n;
		(r === void 0 || r === 0) && (r = 1);
		let i = m(e), a = () => {
			e.read((e) => {
				o(t, e), c.set(i, setTimeout(a, r));
			}, () => {
				c.set(i, setTimeout(a, r));
			});
		};
		c.set(i, setTimeout(a, r));
	}, n.stopReadOnInterval = (e) => {
		let t = m(e), n = c.get(t);
		n !== void 0 && (clearTimeout(n), c.delete(t));
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
//#endregion
//#region src/index.ts
var v = _();
Object.assign(v, {
	createBrowserPrint: _,
	DEFAULT_TIMEOUT_MS: e,
	resolveBaseUrl: t
}), typeof globalThis < "u" && (globalThis.BrowserPrint = v);
//#endregion
export { v as default };

//# sourceMappingURL=browserprint.js.map