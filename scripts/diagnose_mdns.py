#!/usr/bin/env python3
"""Inspect DNS-SD printer records without sending a print job (stdlib only)."""
import argparse
import json
import select
import socket
import struct
import time

SERVICES = ("_ipp._tcp.local.", "_universal._sub._ipp._tcp.local.", "_print._sub._ipp._tcp.local.", "_printer._tcp.local.")


def encode_name(name):
    labels = name.rstrip('.').split('.')
    if any(not label or len(label.encode('utf-8')) > 63 for label in labels):
        raise ValueError('Invalid DNS label')
    return b''.join(bytes([len(label.encode('utf-8'))]) + label.encode('utf-8') for label in labels) + b'\0'


def read_name(data, offset):
    labels, seen, end = [], set(), None
    while True:
        if offset in seen or offset >= len(data):
            raise ValueError('DNS pointer loop or truncated name')
        seen.add(offset)
        size = data[offset]
        if size & 0xc0 == 0xc0:
            if offset + 2 > len(data):
                raise ValueError('Truncated DNS pointer')
            if end is None:
                end = offset + 2
            offset = struct.unpack_from('>H', data, offset)[0] & 0x3fff
        elif size & 0xc0:
            raise ValueError('Unsupported DNS label')
        elif size == 0:
            return '.'.join(labels) + '.', end if end is not None else offset + 1
        else:
            offset += 1
            if offset + size > len(data):
                raise ValueError('Truncated DNS label')
            labels.append(data[offset:offset + size].decode('utf-8', 'replace'))
            offset += size
            if sum(len(label) + 1 for label in labels) > 255:
                raise ValueError('Oversized DNS name')


def parse_packet(data):
    if len(data) < 12:
        raise ValueError('Truncated DNS header')
    _, flags, questions, answers, authority, additional = struct.unpack_from('>6H', data)
    if not flags & 0x8000:
        return []
    offset = 12
    for _ in range(questions):
        _, offset = read_name(data, offset)
        offset += 4
    records = []
    for _ in range(answers + authority + additional):
        name, offset = read_name(data, offset)
        if offset + 10 > len(data):
            raise ValueError('Truncated DNS record')
        kind, _, ttl, length = struct.unpack_from('>HHIH', data, offset)
        offset += 10
        end = offset + length
        if end > len(data):
            raise ValueError('Truncated DNS record data')
        value = None
        if kind == 12:
            value, _ = read_name(data, offset)
        elif kind == 33 and length >= 7:
            priority, weight, port = struct.unpack_from('>HHH', data, offset)
            host, _ = read_name(data, offset + 6)
            value = {'host': host, 'port': port, 'priority': priority, 'weight': weight}
        elif kind == 16:
            value = {}
            cursor = offset
            while cursor < end:
                size = data[cursor]
                cursor += 1
                if cursor + size > end:
                    raise ValueError('Truncated TXT property')
                text = data[cursor:cursor + size].decode('utf-8', 'replace')
                key, _, val = text.partition('=')
                value[key] = val
                cursor += size
        elif kind == 1 and length == 4:
            value = socket.inet_ntop(socket.AF_INET, data[offset:end])
        elif kind == 28 and length == 16:
            value = socket.inet_ntop(socket.AF_INET6, data[offset:end])
        if value is not None:
            records.append({'name': name, 'type': kind, 'ttl': ttl, 'value': value})
        offset = end
    return records


def scan(seconds, interface):
    records = {}
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.bind((interface or '0.0.0.0', 0))
        if interface:
            sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_IF, socket.inet_aton(interface))
        sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_TTL, 255)
        deadline, next_query = time.monotonic() + seconds, 0
        while time.monotonic() < deadline:
            if time.monotonic() >= next_query:
                for service in SERVICES:
                    query = struct.pack('>6H', 0, 0, 1, 0, 0, 0) + encode_name(service) + struct.pack('>HH', 12, 0x8001)
                    sock.sendto(query, ('224.0.0.251', 5353))
                next_query = time.monotonic() + 2
            if not select.select([sock], [], [], min(0.5, max(0, deadline - time.monotonic())))[0]:
                continue
            packet, source = sock.recvfrom(65535)
            try:
                for record in parse_packet(packet):
                    record['source'] = source[0]
                    key = (record['name'], record['type'], json.dumps(record['value'], sort_keys=True))
                    if record['ttl'] == 0:
                        records.pop(key, None)
                    else:
                        records[key] = record
            except (ValueError, struct.error) as error:
                print('Ignored malformed DNS response:', error)
    return list(records.values())


def self_test():
    name = encode_name('_ipp._tcp.local.')
    target = b'\x05Audit\xc0\x0c'
    packet = struct.pack('>6H', 0, 0x8400, 0, 1, 0, 0) + name + struct.pack('>HHIH', 12, 1, 120, len(target)) + target
    records = parse_packet(packet)
    assert records[0]['value'] == 'Audit._ipp._tcp.local.'
    assert records[0]['ttl'] == 120
    try:
        read_name(b'\xc0\x00', 0)
        raise AssertionError('Pointer cycle accepted')
    except ValueError:
        pass
    for length in range(len(packet)):
        try:
            parse_packet(packet[:length])
            raise AssertionError('Truncated packet accepted')
        except (ValueError, struct.error):
            pass
    print('DNS parser self-test passed')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seconds', type=float, default=8)
    parser.add_argument('--interface', help='Local IPv4 address of the LAN interface')
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        if not 0 < args.seconds <= 60:
            parser.error('--seconds must be between 0 and 60')
        result = scan(args.seconds, args.interface)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        if not result:
            print('No response observed. Check sharing, the LAN interface, multicast isolation and firewall rules. This does not prove AirPrint incompatibility.')
