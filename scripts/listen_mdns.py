#!/usr/bin/env python3
"""Listen directly on multicast 224.0.0.251:5353 and print any mDNS packets seen."""
import socket
import struct
import sys
import time

seconds = float(sys.argv[1]) if len(sys.argv) > 1 else 10
interface = sys.argv[2] if len(sys.argv) > 2 else "0.0.0.0"

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
sock.bind(("", 5353))
mreq = socket.inet_aton("224.0.0.251") + socket.inet_aton(interface)
sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, mreq)
if interface != "0.0.0.0":
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_IF, socket.inet_aton(interface))
sock.settimeout(0.5)


def encode_name(name):
    out = b""
    for label in name.rstrip(".").split("."):
        out += bytes([len(label)]) + label.encode()
    return out + b"\0"


# Send a multicast query for IPP printer services.
query = struct.pack(">6H", 0, 0, 1, 0, 0, 0)
for service in ("_ipp._tcp.local.", "_printer._tcp.local."):
    query = struct.pack(">6H", 0, 0, 1, 0, 0, 0) + encode_name(service) + struct.pack(">HH", 12, 1)
    sock.sendto(query, ("224.0.0.251", 5353))

deadline = time.monotonic() + seconds
seen = 0
while time.monotonic() < deadline:
    try:
        packet, source = sock.recvfrom(65535)
    except socket.timeout:
        continue
    seen += 1
    interesting = b"_ipp" in packet or b"DocuPrint" in packet or b"printer" in packet.lower()
    print(f"packet #{seen} from {source[0]}:{source[1]} len={len(packet)} ipp={interesting}")

print(f"total packets seen: {seen}")
