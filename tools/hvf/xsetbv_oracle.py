#!/usr/bin/env python3
"""Execute real XSETBV/#UD/#GP instructions in a bounded QEMU TCG guest.

Compare raw records with the actual host-independent HVF decision/commit module.
No native Intel VMX execution or full CPU-profile equivalence is claimed.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--work-dir', type=Path, required=True)
    parser.add_argument('--llvm-bin', type=Path, required=True)
    parser.add_argument('--qemu', default='qemu-system-i386')
    parser.add_argument('--module', type=Path)
    args = parser.parse_args()
    out = args.work_dir.resolve()
    out.mkdir(parents=True, exist_ok=True)
    owner = out / '.rax-hvf-xsetbv'
    if any(out.iterdir()) and (not owner.is_file() or owner.read_text() != 'version=1\n'):
        raise ValueError('Use an empty or owned version1 directory')
    owner.write_text('version=1\n')
    module = (args.module or args.repo / 'src/backend/hvf/xsetbv.rs').resolve()
    guest = Path(__file__).with_name('xsetbv_guest.s')
    values = [*range(16), 7 | (1 << 31), 7 | (1 << 32), 7 | (1 << 63)]
    cases = [(enabled << 18, index, value & 0xffff_ffff, value >> 32)
             for enabled in range(2) for index in [0, 1, 0xffff_ffff] for value in values]
    (out / 'cases.bin').write_bytes(b''.join(struct.pack('<IIII', *case) for case in cases))
    (out / 'guest.s').write_text(guest.read_text().replace('@CASE_COUNT@', str(len(cases))))
    (out / 'link.ld').write_text('ENTRY(_start)\nSECTIONS {. = 0x100000; .multiboot : {*(.multiboot)} .text : {*(.text*)} .rodata : {*(.rodata*)} .data : {*(.data*)} .bss : {*(.bss*) *(COMMON)}}\n')
    # Include the actual product module; its fallible access trait is exercised,
    # including fault preservation and success commit order, on the same records.
    rust = '#[path=' + json.dumps(str(module)) + '] mod xsetbv;\n' + r'''
use xsetbv::*;
struct Access { input:XsetbvInput, xcr0:u64, rip:u64, fault:u32, events:Vec<u8> }
impl XsetbvAccess for Access {
 type Error=();
 fn input(&mut self)->Result<XsetbvInput,()> {self.events.push(0);Ok(self.input)}
 fn write_xcr0(&mut self,v:u64)->Result<(),()> {self.events.push(1);self.xcr0=v;Ok(())}
 fn advance_rip(&mut self)->Result<(),()> {self.events.push(2);self.rip+=3;Ok(())}
 fn inject_fault(&mut self,f:XsetbvFault)->Result<(),()> {self.events.push(3);self.fault=match f {XsetbvFault::UndefinedOpcode=>6,XsetbvFault::GeneralProtection=>13};Ok(())}
}
fn word(b:&[u8],i:usize)->u32 {u32::from_le_bytes(b[i..i+4].try_into().unwrap())}
fn main() {
 let a:Vec<_>=std::env::args().collect();let cases=std::fs::read(&a[1]).unwrap();let raw=std::fs::read(&a[2]).unwrap();let n=cases.len()/16;
 assert_eq!(cases.len()%16,0);assert_eq!(raw.len(),20+n*24);assert_eq!(&raw[..8],b"RAXXCR01");assert_eq!(word(&raw,8) as usize,n);assert_eq!(&raw[12+n*24..],b"XCR0DONE");
 let mut failed=0;let mut order_failed=0;let mut cpuid_failed=0;let mut denied=0;
 for (index,c) in cases.chunks_exact(16).enumerate() {
  let mut io=Access {input:XsetbvInput {xsave_supported:true,supported_xcr0:7,cr0:1,cr4:u64::from(word(c,0)),rflags:2,cs_selector:8,rcx:u64::from(word(c,4)),rax:u64::from(word(c,8)),rdx:u64::from(word(c,12))},xcr0:3,rip:0x1000,fault:0,events:vec![]};
  handle_xsetbv(&mut io).unwrap();let r=&raw[12+index*24..12+(index+1)*24];let expected=(word(r,0),word(r,4),u64::from(word(r,8))|(u64::from(word(r,12))<<32));
  let fault=expected.0!=0;denied+=usize::from(fault);let expected_events=if fault {vec![0,3]}else{vec![0,1,2]};
  if io.events!=expected_events {order_failed+=1;}
  let features=xsave_cpuid(1,0,(0,0,(1<<26)|(1<<27),0),io.input.cr4,io.xcr0).2&(1<<27);
  let size=xsave_cpuid(0x0d,0,(7,832,832,0),io.input.cr4,io.xcr0).1;
  if features!=word(r,16) || size!=word(r,20) {cpuid_failed+=1;}
  if (io.fault,0,io.xcr0)!=expected || io.rip!=if fault{0x1000}else{0x1003} {failed+=1;eprintln!("case={index} got={:?} expected={expected:?} rip={:x} events={:?}",(io.fault,0,io.xcr0),io.rip,io.events);}
 }
 println!("{{\"cases\":{n},\"denied\":{denied},\"failed\":{failed},\"commit_order_failed\":{order_failed},\"cpuid_failed\":{cpuid_failed}}}");if failed!=0 || order_failed!=0 || cpuid_failed!=0 {std::process::exit(1)}
}
'''
    (out / 'compare.rs').write_text(rust)
    manifest = {'version': 1, 'cases': len(cases), 'tool_sha256': sha(Path(__file__)),
                'guest_source_sha256': sha(guest), 'module_sha256': sha(module),
                'commands': [], 'scope': 'CPL0 XSAVE-enabled CPU; OSXSAVE0/1, index/value faults and XCR0 preservation. TCG, not VMX.'}
    def run(name, command, expected=0):
        start = time.monotonic()
        try:
            result = subprocess.run(command, cwd=out, text=True, stdout=subprocess.PIPE,
                                    stderr=subprocess.PIPE, timeout=60)
        except subprocess.TimeoutExpired:
            manifest['commands'].append({'name': name, 'args': command, 'code': 'timeout',
                                         'seconds': time.monotonic() - start})
            (out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
            raise RuntimeError(f'{name} exceeded60s; no passing result') from None
        (out / (name + '.stdout')).write_text(result.stdout)
        (out / (name + '.stderr')).write_text(result.stderr)
        manifest['commands'].append({'name': name, 'args': command, 'code': result.returncode,
                                     'seconds': time.monotonic() - start})
        (out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        if expected is not None and result.returncode != expected:
            raise RuntimeError(f'{name}: code{result.returncode}; see retained diagnostics')
        return result
    llvm = args.llvm_bin.resolve()
    run('qemu-version', [args.qemu, '--version'])
    run('llvm-version', [str(llvm / 'llvm-mc'), '--version'])
    run('rust-version', ['rustc', '+stable', '-vV'])
    run('assemble', [str(llvm / 'llvm-mc'), '-triple=i386-none-elf', '-filetype=obj', 'guest.s', '-o', 'guest.o'])
    run('link', [str(llvm / 'ld.lld'), '-m', 'elf_i386', '-T', 'link.ld', 'guest.o', '-o', 'guest.elf'])
    run('qemu', [args.qemu, '-machine', 'pc', '-cpu', 'Haswell-noTSX', '-accel', 'tcg,thread=single', '-m', '64M', '-nodefaults', '-display', 'none', '-monitor', 'none', '-serial', 'none', '-debugcon', 'file:' + str(out / 'results.bin'), '-device', 'isa-debug-exit,iobase=0xf4,iosize=0x04', '-no-reboot', '-kernel', str(out / 'guest.elf')], 33)
    run('compare-build', ['rustc', '+stable', '--edition=2024', 'compare.rs', '-o', 'compare'])
    result = run('compare', [str(out / 'compare'), str(out / 'cases.bin'), str(out / 'results.bin')], expected=None)
    assert result.returncode in (0, 1)
    report = json.loads(result.stdout)
    assert report['cases'] == 114 and report['denied'] == 111, report
    assert result.returncode == int(report['failed'] != 0 or report['commit_order_failed'] != 0 or report['cpuid_failed'] != 0)
    assert sha(module) == manifest['module_sha256']
    manifest['artifact_sha256'] = {name: sha(out / name) for name in ['cases.bin', 'results.bin', 'guest.s', 'guest.elf', 'compare.rs']}
    manifest['report'] = report
    (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(report))
    return result.returncode


if __name__ == '__main__':
    raise SystemExit(main())
