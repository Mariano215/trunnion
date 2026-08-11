// Origin-only client for the read-only console API in docs/CONSOLE-API.md.
//
// Every failure surfaces as an ApiFault carrying cause and fix, because the
// API's error body is a Fault and an error that does not name the action to
// take is not an error message this project ships.

export class ApiFault extends Error {
  constructor(cause, fix, status, path) {
    super(cause);
    this.name = 'ApiFault';
    this.cause_ = cause;
    this.fix = fix;
    this.status = status;
    this.path = path;
  }
}

// Same-origin only. A relative path can never reach another host, and the
// console has no reason to.
async function get(path) {
  let res;
  try {
    res = await fetch(path, { headers: { accept: 'application/json' } });
  } catch (e) {
    throw new ApiFault(
      `the console could not reach ${path}: ${e.message}`,
      'check that the gantry server is still running and serving this origin, then reload',
      0,
      path,
    );
  }
  let body = null;
  const text = await res.text();
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      body = null;
    }
  }
  if (!res.ok) {
    const cause = (body && body.cause) || `${path} returned HTTP ${res.status}`;
    const fix = (body && body.fix) || 'read the server log for the failing read, then reload';
    throw new ApiFault(cause, fix, res.status, path);
  }
  if (body === null) {
    throw new ApiFault(
      `${path} returned a body that is not JSON`,
      'confirm the request reached the gantry API and not a proxy or a static file',
      res.status,
      path,
    );
  }
  return body;
}

function qs(params) {
  const u = new URLSearchParams();
  for (const [k, v] of Object.entries(params || {})) {
    if (v === undefined || v === null || v === '') continue;
    if (Array.isArray(v)) v.forEach((x) => u.append(k, x));
    else u.append(k, v);
  }
  const s = u.toString();
  return s ? `?${s}` : '';
}

export const api = {
  // The workspace routes. A console started without a ledger still answers
  // these, because the question a review opens with is about a set of
  // repositories and not about one log.
  projects: () => get('/api/projects'),
  projectScan: (id) => get(`/api/projects/${encodeURIComponent(id)}/scan`),
  projectRemediate: (id) => get(`/api/projects/${encodeURIComponent(id)}/remediate`),
  score: () => get('/api/score'),
  head: () => get('/api/head'),
  events: (params) => get(`/api/events${qs(params)}`),
  event: (id) => get(`/api/events/${encodeURIComponent(id)}`),
  runs: () => get('/api/runs'),
  policy: () => get('/api/policy'),
  trust: () => get('/api/trust'),
  approvals: () => get('/api/approvals'),
  verify: () => get('/api/verify'),
};

// Shared, mutable, deliberately small. The verification result lives here
// because every view has to know the ledger failed.
export const state = {
  verify: null,        // the last /api/verify body
  verifyError: null,   // an ApiFault if the route could not be read at all
  faultIds: new Set(), // event ids named in verify.faults
  acknowledged: false, // the takeover was dismissed by a deliberate click
  head: null,
  noLedger: false,     // this console was started without one
};

export function recordVerify(body) {
  state.verify = body;
  state.verifyError = null;
  state.noLedger = false;
  state.faultIds = new Set((body.faults || []).map((f) => f.id).filter(Boolean));
  if (body.ok) state.acknowledged = false;
  return body;
}

// "There is no log here" and "the log here is damaged" are different states,
// and only the second is an alarm. A console started with a ledger always has
// /api/verify as a route, so a 404 on it is the first state and never the
// second: the workspace answers, the ledger views say what is missing, and the
// verification takeover stays out of the way.
export function recordVerifyError(err) {
  state.verify = null;
  state.faultIds = new Set();
  if (err && err.status === 404) {
    state.noLedger = true;
    state.verifyError = null;
    return;
  }
  state.noLedger = false;
  state.verifyError = err;
}

// The only two questions the rest of the console asks about verification.
export const ledgerBroken = () => state.verify !== null && state.verify.ok === false;
export const verifyUnknown = () => state.verify === null;
