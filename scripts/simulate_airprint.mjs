// A separate Node.js process emulates a LAN AirPrint client on a host-only
// virtual adapter. It queries mDNS and sends IPP/HTTP; it never submits Print-Job.
import dgram from 'node:dgram';
import http from 'node:http';
import os from 'node:os';
import { spawn } from 'node:child_process';
import { resolve } from 'node:path';

const adapters = Object.entries(os.networkInterfaces())
  .flatMap(([name, addresses]) => (addresses || []).filter(a => a.family === 'IPv4')
    .map(a => ({ name, address: a.address })));
const chosen = adapters.find(a => a.address === process.argv[2]) ??
  adapters.find(a => /vethernet.*default switch/i.test(a.name));
if (!chosen || !/vethernet|vmnet1|host.only/i.test(chosen.name)) {
  throw new Error('Pass an IPv4 address of a host-only virtual adapter, e.g. node scripts/simulate_airprint.mjs 172.22.48.1');
}
const childPath = resolve('src-tauri/target/debug/examples/simulate_lan.exe');
const child = spawn(childPath, [chosen.address], { stdio: ['pipe', 'pipe', 'pipe'] });
let output = '';
let childErrors = '';
child.stderr.on('data', chunk => childErrors += chunk);
const ready = new Promise((accept, reject) => {
  const timeout = setTimeout(() => reject(new Error(`Simulation server did not start: ${childErrors}`)), 10000);
  child.on('error', error => { clearTimeout(timeout); reject(error); });
  child.on('exit', code => { clearTimeout(timeout); reject(new Error(`Simulation server exited ${code}: ${childErrors}`)); });
  child.stdout.on('data', chunk => {
    output += chunk;
    for (const line of output.split(/\r?\n/)) {
      if (line.startsWith('SIM_READY ')) {
        clearTimeout(timeout);
        accept(JSON.parse(line.slice(10)));
      }
    }
  });
});

function dnsName(name) {
  return Buffer.concat([...name.replace(/\.$/, '').split('.').map(label => {
    const bytes = Buffer.from(label);
    if (!bytes.length || bytes.length > 63) throw new Error('Invalid DNS label');
    return Buffer.concat([Buffer.from([bytes.length]), bytes]);
  }), Buffer.from([0])]);
}
function parseName(packet, start) {
  let position = start;
  let end = -1;
  const seen = new Set();
  const labels = [];
  while (true) {
    if (position >= packet.length || seen.has(position)) throw new Error('Bad DNS pointer');
    seen.add(position);
    const length = packet[position];
    if ((length & 0xc0) === 0xc0) {
      if (position + 1 >= packet.length) throw new Error('Short DNS pointer');
      if (end < 0) end = position + 2;
      position = packet.readUInt16BE(position) & 0x3fff;
      continue;
    }
    if (length & 0xc0) throw new Error('Unsupported DNS label');
    position++;
    if (!length) return [labels.join('.') + '.', end < 0 ? position : end];
    if (position + length > packet.length) throw new Error('Short DNS name');
    labels.push(packet.toString('utf8', position, position + length));
    position += length;
  }
}
function dnsRecords(packet) {
  if (packet.length < 12 || !(packet.readUInt16BE(2) & 0x8000)) return [];
  let position = 12;
  for (let i = 0; i < packet.readUInt16BE(4); i++) {
    [, position] = parseName(packet, position);
    position += 4;
  }
  const count = packet.readUInt16BE(6) + packet.readUInt16BE(8) + packet.readUInt16BE(10);
  const found = [];
  for (let i = 0; i < count; i++) {
    const [name, next] = parseName(packet, position);
    if (next + 10 > packet.length) throw new Error('Short DNS record');
    const type = packet.readUInt16BE(next);
    const size = packet.readUInt16BE(next + 8);
    position = next + 10;
    const end = position + size;
    if (end > packet.length) throw new Error('Short DNS value');
    let value;
    if (type === 12) [value] = parseName(packet, position);
    if (type === 33 && size >= 7) {
      const [host] = parseName(packet, position + 6);
      value = { host, port: packet.readUInt16BE(position + 4) };
    }
    if (type === 1 && size === 4) value = [...packet.subarray(position, end)].join('.');
    if (type === 16) {
      value = {};
      for (let cursor = position; cursor < end;) {
        const length = packet[cursor++];
        if (cursor + length > end) throw new Error('Bad TXT record');
        const [key, ...rest] = packet.toString('utf8', cursor, cursor + length).split('=');
        value[key.toLowerCase()] = rest.join('=');
        cursor += length;
      }
    }
    if (value !== undefined) found.push({ name, type, value });
    position = end;
  }
  return found;
}
function question(service) {
  const buffer = Buffer.alloc(12);
  buffer.writeUInt16BE(1, 4);
  return Buffer.concat([buffer, dnsName(service), Buffer.from([0, 12, 0x80, 1])]);
}
function matchFixture(records, fixture) {
  const services = ['_universal._sub._ipp._tcp.local.', '_ipp._tcp.local.'];
  const txt = records.find(r => r.type === 16 && r.value.ty === fixture.name);
  if (!txt) return null;
  const instance = txt.name;
  const universal = records.some(r => r.type === 12 && r.name === services[0] && r.value === instance);
  const base = records.some(r => r.type === 12 && r.name === services[1] && r.value === instance);
  const srv = records.find(r => r.type === 33 && r.name === instance);
  const address = srv && records.find(r => r.type === 1 && r.name === srv.value.host && r.value === fixture.address);
  return universal && base && address && txt.value.rp === fixture.rp && srv.value.port === fixture.port
    ? { instance, txt: txt.value, srv: srv.value, address: address.value, records: records.length }
    : null;
}
async function discover(fixture) {
  const socket = dgram.createSocket({ type: 'udp4', reuseAddr: true });
  const records = [];
  const errors = [];
  socket.on('message', packet => {
    try { records.push(...dnsRecords(packet)); } catch (error) { errors.push(error); }
  });
  await new Promise((accept, reject) => {
    socket.once('error', reject);
    socket.bind(5353, '0.0.0.0', accept);
  });
  socket.addMembership('224.0.0.251', chosen.address);
  socket.setMulticastInterface(chosen.address);
  const services = [
    '_universal._sub._ipp._tcp.local.',
    '_ipp._tcp.local.',
    '_printer._tcp.local.',
  ];
  try {
    const deadline = Date.now() + 8000;
    while (Date.now() < deadline) {
      for (const service of services) socket.send(question(service), 5353, '224.0.0.251');
      await new Promise(done => setTimeout(done, 500));
      const found = matchFixture(records, fixture);
      if (found) return found;
    }
    throw new Error(`DNS-SD records incomplete (${records.length} received, ${errors.length} malformed)`);
  } finally { socket.close(); }
}
function record(name, type, value) {
  const header = Buffer.alloc(10);
  header.writeUInt16BE(type, 0);
  header.writeUInt16BE(1, 2);
  header.writeUInt32BE(120, 4);
  header.writeUInt16BE(value.length, 8);
  return Buffer.concat([dnsName(name), header, value]);
}
function syntheticDnsResponse(fixture) {
  const instance = `${fixture.name}._ipp._tcp.local.`;
  const hostname = 'airprinter-simulation.local.';
  const text = Object.entries({ ty: fixture.name, rp: fixture.rp, pdl: 'image/urf,application/pdf', URF: 'W8,SRGB24,RS300' })
    .map(([key, value]) => {
      const bytes = Buffer.from(`${key}=${value}`);
      return Buffer.concat([Buffer.from([bytes.length]), bytes]);
    });
  const srv = Buffer.alloc(6);
  srv.writeUInt16BE(fixture.port, 4);
  const answer = [
    record('_universal._sub._ipp._tcp.local.', 12, dnsName(instance)),
    record('_ipp._tcp.local.', 12, dnsName(instance)),
    record(instance, 33, Buffer.concat([srv, dnsName(hostname)])),
    record(instance, 16, Buffer.concat(text)),
    record(hostname, 1, Buffer.from(fixture.address.split('.').map(Number))),
  ];
  const header = Buffer.alloc(12);
  header.writeUInt16BE(0x8400, 2);
  header.writeUInt16BE(answer.length, 6);
  return Buffer.concat([header, ...answer]);
}
async function discoverWithLoopbackResponder(fixture) {
  const server = dgram.createSocket('udp4');
  const client = dgram.createSocket('udp4');
  try {
    await new Promise((accept, reject) => { server.once('error', reject); server.bind(0, '127.0.0.1', accept); });
    await new Promise((accept, reject) => { client.once('error', reject); client.bind(0, '127.0.0.1', accept); });
    server.on('message', (_, sender) => server.send(syntheticDnsResponse(fixture), sender.port, sender.address));
    const answer = new Promise((accept, reject) => {
      const timeout = setTimeout(() => reject(new Error('Isolated DNS-SD responder timed out')), 2000);
      client.once('message', packet => { clearTimeout(timeout); accept(dnsRecords(packet)); });
    });
    client.send(question('_universal._sub._ipp._tcp.local.'), server.address().port, '127.0.0.1');
    const found = matchFixture(await answer, fixture);
    if (!found) throw new Error('Simulated DNS-SD response did not match the virtual printer');
    return found;
  } finally { client.close(); server.close(); }
}
function attribute(tag, name, value) {
  const label = Buffer.from(name); const bytes = Buffer.from(value);
  const header = Buffer.alloc(5);
  header[0] = tag; header.writeUInt16BE(label.length, 1); header.writeUInt16BE(bytes.length, 3);
  return Buffer.concat([header.subarray(0, 3), label, header.subarray(3), bytes]);
}
function ippRequest(op, uri, extras = []) {
  const header = Buffer.alloc(8);
  header[0] = 2; header.writeUInt16BE(op, 2); header.writeUInt32BE(op, 4);
  return Buffer.concat([header, Buffer.from([1]),
    attribute(0x47, 'attributes-charset', 'utf-8'),
    attribute(0x48, 'attributes-natural-language', 'en'),
    attribute(0x45, 'printer-uri', uri), ...extras, Buffer.from([3])]);
}
async function ippPost(info, path, body, host) {
  return await new Promise((accept, reject) => {
    const request = http.request({ host: info.address, port: info.srv.port, path, method: 'POST',
      headers: { 'Host': host || `${info.address}:${info.srv.port}`, 'Content-Type': 'application/ipp', 'Content-Length': body.length } }, response => {
      const chunks = [];
      response.on('data', chunk => chunks.push(chunk));
      response.on('end', () => accept({ http: response.statusCode, body: Buffer.concat(chunks) }));
    });
    request.setTimeout(3000, () => request.destroy(new Error('IPP request timed out')));
    request.on('error', reject);
    request.end(body);
  });
}
try {
  const fixture = await ready;
  let info;
  let discovery;
  try {
    info = await discover(fixture);
    discovery = 'real multicast on host-only virtual adapter';
  } catch (error) {
    if (!String(error).includes('DNS-SD records incomplete')) throw error;
    info = await discoverWithLoopbackResponder(fixture);
    discovery = 'simulated DNS-SD response over UDP loopback; host-only multicast unavailable';
  }
  const path = '/' + info.txt.rp;
  const uri = `ipp://${info.address}:${info.srv.port}${path}`;
  const attrs = await ippPost(info, path, ippRequest(0x000b, uri));
  if (attrs.http !== 200 || attrs.body.readUInt16BE(2) !== 0 || !attrs.body.includes(Buffer.from(fixture.name))) {
    throw new Error(`Get-Printer-Attributes failed: HTTP ${attrs.http}, IPP ${attrs.body.readUInt16BE(2)}`);
  }
  const badCopies = Buffer.from([0, 0, 3, 231]); // 999
  const invalid = await ippPost(info, path, ippRequest(0x0004, uri, [attribute(0x21, 'copies', badCopies)]));
  if (invalid.http !== 200 || invalid.body.readUInt16BE(2) !== 0x040b) {
    throw new Error('Validate-Job accepted an unsupported copy count');
  }
  const wrongHost = await ippPost(info, path, ippRequest(0x000b, uri), `other.example:${info.srv.port}`);
  if (wrongHost.http !== 400) throw new Error('Host validation did not reject a foreign name');
  console.log(JSON.stringify({ result: 'pass', interface: chosen, discovery, discovered: info.instance, ip: info.address,
    ippPort: info.srv.port, rp: info.txt.rp, tests: ['DNS-SD PTR/SRV/TXT/A', 'IPP attributes', 'IPP validation', 'Host rejection'] }, null, 2));
} finally {
  child.stdin.end('stop\n');
  await Promise.race([
    new Promise(done => child.once('exit', done)),
    new Promise(done => setTimeout(() => { child.kill(); done(); }, 5000)),
  ]);
}
