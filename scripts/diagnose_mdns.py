#!/usr/bin/env python3
"""
AirPrinter mDNS 诊断工具
用于检查打印机是否在网络上正确广播
"""

import socket
import struct
import sys
import time
import threading
from datetime import datetime

# mDNS 多播地址和端口
MDNS_ADDR = "224.0.0.251"
MDNS_PORT = 5353

# AirPrint 服务类型
SERVICE_TYPES = [
    b"_ipp._tcp.local",
    b"_ipps._tcp.local",
    b"_printer._tcp.local",
]

def create_mdns_query(service_type: bytes) -> bytes:
    """创建 mDNS 查询包"""
    # Transaction ID
    tid = 0x0000
    # Flags: Standard query
    flags = 0x0000
    # Questions: 1
    questions = 1
    # Answer RRs: 0
    answer_rrs = 0
    # Authority RRs: 0
    authority_rrs = 0
    # Additional RRs: 0
    additional_rrs = 0
    
    header = struct.pack(
        ">HHHHHH",
        tid, flags, questions, answer_rrs, authority_rrs, additional_rrs
    )
    
    # Query name
    labels = service_type.split(b".")
    qname = b"".join(bytes([len(label)]) + label for label in labels if label) + b"\x00"
    
    # Query type: PTR (12)
    qtype = 12
    # Query class: IN (1), with unicast response bit
    qclass = 0x8001
    
    query = struct.pack(">H", qtype) + struct.pack(">H", qclass)
    
    return header + qname + query


def parse_mdns_packet(data: bytes, addr: tuple) -> dict:
    """解析 mDNS 响应包"""
    if len(data) < 12:
        return None
    
    tid, flags, questions, answer_rrs, authority_rrs, additional_rrs = struct.unpack(">HHHHHH", data[:12])
    
    result = {
        "from": addr[0],
        "tid": tid,
        "flags": flags,
        "answers": [],
        "services": []
    }
    
    offset = 12
    
    # 跳过问题部分
    for _ in range(questions):
        while offset < len(data):
            length = data[offset]
            if length == 0:
                offset += 1
                break
            if length & 0xC0 == 0xC0:
                offset += 2
                break
            offset += length + 1
        offset += 4  # Skip QTYPE and QCLASS
    
    # 解析应答部分
    for _ in range(answer_rrs):
        name_parts = []
        while offset < len(data):
            length = data[offset]
            if length == 0:
                offset += 1
                break
            if length & 0xC0 == 0xC0:
                # Compression pointer
                ptr = struct.unpack(">H", data[offset:offset+2])[0] & 0x3FFF
                # 简化处理，这里不递归解析
                offset += 2
                break
            offset += 1
            name_parts.append(data[offset:offset+length].decode('utf-8', errors='ignore'))
            offset += length
        
        if offset + 10 > len(data):
            break
            
        rtype, rclass, ttl = struct.unpack(">HHI", data[offset:offset+10])
        offset += 10
        
        rdlength = struct.unpack(">H", data[offset:offset+2])[0]
        offset += 2
        
        rdata = data[offset:offset+rdlength]
        offset += rdlength
        
        if rtype == 12:  # PTR record
            # 解析 PTR 数据
            ptr_parts = []
            rd_offset = 0
            while rd_offset < len(rdata):
                length = rdata[rd_offset]
                if length == 0:
                    break
                if length & 0xC0 == 0xC0:
                    break
                rd_offset += 1
                ptr_parts.append(rdata[rd_offset:rd_offset+length].decode('utf-8', errors='ignore'))
                rd_offset += length
            
            service_name = ".".join(name_parts)
            ptr_value = ".".join(ptr_parts)
            result["services"].append({
                "type": service_name,
                "name": ptr_value,
                "ttl": ttl
            })
    
    return result


def scan_airprint(timeout: int = 5):
    """扫描网络上的 AirPrint 服务"""
    print(f"[{datetime.now().strftime('%H:%M:%S')}] 开始扫描 AirPrint 服务...")
    print(f"[{datetime.now().strftime('%H:%M:%S')}] 超时时间: {timeout}秒")
    print("-" * 60)
    
    # 创建 UDP 套接字
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(timeout)
    
    # 允许多播
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    
    # 绑定到 mDNS 端口
    try:
        sock.bind(("0.0.0.0", 0))  # 使用随机端口发送
    except socket.error as e:
        print(f"绑定失败: {e}")
        return
    
    found_services = {}
    start_time = time.time()
    
    # 发送查询
    for service_type in SERVICE_TYPES:
        query = create_mdns_query(service_type)
        sock.sendto(query, (MDNS_ADDR, MDNS_PORT))
        print(f"[{datetime.now().strftime('%H:%M:%S')}] 查询: {service_type.decode()}")
    
    # 接收响应
    while time.time() - start_time < timeout:
        try:
            data, addr = sock.recvfrom(4096)
            result = parse_mdns_packet(data, addr)
            
            if result and result["services"]:
                for svc in result["services"]:
                    key = f"{svc['name']}@{addr[0]}"
                    if key not in found_services:
                        found_services[key] = {
                            "name": svc["name"],
                            "ip": addr[0],
                            "type": svc["type"],
                            "ttl": svc["ttl"]
                        }
                        print(f"\n[{datetime.now().strftime('%H:%M:%S')}] 发现服务!")
                        print(f"  名称: {svc['name']}")
                        print(f"  类型: {svc['type']}")
                        print(f"  IP: {addr[0]}")
                        print(f"  TTL: {svc['ttl']}秒")
                        
        except socket.timeout:
            break
        except Exception as e:
            print(f"错误: {e}")
    
    sock.close()
    
    print("-" * 60)
    print(f"[{datetime.now().strftime('%H:%M:%S')}] 扫描完成")
    print(f"共发现 {len(found_services)} 个服务")
    
    if not found_services:
        print("\n⚠️ 没有发现任何 AirPrint 服务!")
        print("\n可能的原因:")
        print("1. AirPrinter 应用程序没有运行")
        print("2. 防火墙阻止了 mDNS 通信 (端口 5353 UDP)")
        print("3. AirPrinter 没有以管理员身份运行 (端口631需要权限)")
        print("4. 网络隔离阻止了多播流量")
    else:
        # 检查是否有 _ipp._tcp 服务
        ipp_services = [s for s in found_services.values() if "_ipp._tcp" in s["type"]]
        if ipp_services:
            print(f"\n✅ 发现 {len(ipp_services)} 个 IPP 服务 (AirPrint 兼容)")
        else:
            print("\n⚠️ 发现服务但没有 IPP 服务")
    
    return found_services


def check_port_631(ip: str) -> bool:
    """检查目标IP的631端口是否开放"""
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(2)
        result = sock.connect_ex((ip, 631))
        sock.close()
        return result == 0
    except:
        return False


def main():
    print("=" * 60)
    print("AirPrinter mDNS 诊断工具")
    print("=" * 60)
    print()
    
    services = scan_airprint(timeout=10)
    
    if services:
        print("\n" + "=" * 60)
        print("检查 IPP 端口 (631) 连通性")
        print("=" * 60)
        
        for key, svc in services.items():
            ip = svc["ip"]
            print(f"\n检查 {ip}:631 ...", end=" ")
            if check_port_631(ip):
                print("✅ 端口开放")
            else:
                print("❌ 端口无法连接")
                print(f"  提示: {svc['name']} 可能没有在监听631端口")
                print(f"  或者防火墙阻止了连接")
    
    print("\n" + "=" * 60)
    print("诊断建议:")
    print("=" * 60)
    print("1. 确保 AirPrinter 以管理员身份运行")
    print("2. 检查 Windows 防火墙是否允许应用通过")
    print("3. 检查第三方杀毒软件是否阻止了网络通信")
    print("4. 尝试暂时关闭防火墙进行测试")
    print()
    input("按回车键退出...")


if __name__ == "__main__":
    main()
