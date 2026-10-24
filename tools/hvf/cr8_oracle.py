#!/usr/bin/env python3
"""Execute real CR8/APIC.TPR instructions in a bounded QEMU TCG guest.

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
    parser.add_argument('--qemu', default='qemu-system-x86_64')
    parser.add_argument('--module', type=Path)
    args = parser.parse_args()
    out = args.work_dir.resolve()
    out.mkdir(parents=True, exist_ok=True)
    owner = out / '.rax-hvf-cr8'
    if any(out.iterdir()) and (not owner.is_file() or owner.read_text() != 'version=1\n'):
        raise ValueError('Use an empty or owned version1 directory')
    owner.write_text('version=1\n')
    module = (args.module or args.repo / 'src/backend/hvf/cr8.rs').resolve()
    guest = Path(__file__).with_name('cr8_guest.s')
    values = [*range(16), *[3 | (1 << bit) for bit in range(4, 64)], 0xffff, 0xffff_ffff, 0xffff_ffff_ffff_ffff]
    cases = [(0, 0, value & 0xffff_ffff, value >> 32) for value in values]
    cases += [(1, 0, value, 0) for value in range(256)]
    (out / 'cases.bin').write_bytes(b''.join(struct.pack('<IIII', *case) for case in cases))
    (out / 'guest.s').write_text(guest.read_text().replace('@CASE_COUNT@', str(len(cases))))
    (out / 'link.ld').write_text('ENTRY(_start)\nSECTIONS {. = 0x100000; .multiboot : {*(.multiboot)} .text : {*(.text*)} .rodata : {*(.rodata*)} .data : {*(.data*)} .bss : {*(.bss*) *(COMMON)}}\n')
    # Include the actual product module; its fallible access trait is exercised,
    # including fault preservation and success commit order, on the same records.
    rust = '#[path=' + json.dumps(str(module)) + '] mod cr8;\n' + r'''
use cr8::*;
struct Host { gpr:u64, tpr:u64, rip:u64, fault:u32, events:Vec<u8> }
impl Cr8Access for Host {
 type Error=();
 fn read_gpr(&mut self,i:u8)->Result<u64,()> {assert_eq!(i,0);self.events.push(0);Ok(self.gpr)}
 fn write_gpr(&mut self,i:u8,v:u64)->Result<(),()> {assert_eq!(i,0);self.events.push(1);self.gpr=v;Ok(())}
 fn read_tpr(&mut self)->Result<u64,()> {self.events.push(2);Ok(self.tpr)}
 fn write_tpr(&mut self,v:u64)->Result<(),()> {self.events.push(3);self.tpr=v;Ok(())}
 fn inject_gp(&mut self)->Result<(),()> {self.events.push(4);self.fault=13;Ok(())}
 fn advance_rip(&mut self)->Result<(),()> {self.events.push(5);self.rip+=4;Ok(())}
}
fn word(b:&[u8],i:usize)->u32 {u32::from_le_bytes(b[i..i+4].try_into().unwrap())}
fn main() {
 let args:Vec<_>=std::env::args().collect();let cases=std::fs::read(&args[1]).unwrap();let raw=std::fs::read(&args[2]).unwrap();let n=cases.len()/16;
 assert_eq!(cases.len()%16,0);assert_eq!(raw.len(),20+n*24);assert_eq!(&raw[..8],b"RAXCR801");assert_eq!(word(&raw,8) as usize,n);assert_eq!(&raw[12+n*24..],b"CR8_DONE");
 let mut local_failed=0;let mut independent_failed=0;let mut raw_differences=0;let mut reference_profile_failed=0;let mut order_failed=0;let mut reserved=0;
 for (index,c) in cases.chunks_exact(16).enumerate() {
  let read=word(c,0)==1;let value=u64::from(word(c,8))|(u64::from(word(c,12))<<32);let fault=!read && value>15;reserved+=usize::from(fault);
  let mut host=Host {gpr:if read {0x1122_3344_5566_7788}else{value},tpr:if read{value}else{0xa7},rip:0x1000,fault:0,events:vec![]};assert!(handle(&mut host,if read{0x18}else{8}).unwrap());
  let r=&raw[12+index*24..12+(index+1)*24];let reference=(word(r,0),word(r,4),u64::from(word(r,8))|(u64::from(word(r,12))<<32),u64::from(word(r,16)));assert_eq!(word(r,20),0);
  let tpr=if read{value}else if fault{0xa7}else{value<<4};let primary=(if fault{13}else{0},0,(tpr>>4)&15,tpr);
  let result=(host.fault,0,if read{host.gpr}else{(host.tpr>>4)&15},host.tpr);
  if result!=primary || host.rip!=if fault{0x1000}else{0x1004} {local_failed+=1;eprintln!("primary case{index}:got={result:?} expected={primary:?}");}
  let order=if read {vec![2,1,5]}else if fault{vec![0,4]}else{vec![0,3,5]};if host.events!=order {order_failed+=1;}
  if result!=reference {raw_differences+=1;}
  if !fault {if result!=reference {independent_failed+=1;}}
  else {
   // Retain QEMU's known reserved-bit masking disagreement. Never change the
   // product's required primary-source #GP path to fit it.
   let known_qemu=(0,0,value&15,(value&15)<<4);
   if reference!=primary && reference!=known_qemu {reference_profile_failed+=1;}
  }
 }
 println!("{{\"cases\":{n},\"independent_legal_cases\":{},\"reserved_primary_cases\":{reserved},\"local_profile_failed\":{local_failed},\"independent_legal_failed\":{independent_failed},\"commit_order_failed\":{order_failed},\"qemu_raw_differences\":{raw_differences},\"unknown_reference_profile_failed\":{reference_profile_failed}}}",n-reserved);
 if local_failed!=0 || independent_failed!=0 || order_failed!=0 || reference_profile_failed!=0 {std::process::exit(1)}
}
'''
    (out / 'compare.rs').write_text(rust)
    manifest = {'version': 1, 'cases': len(cases), 'tool_sha256': sha(Path(__file__)),
                'guest_source_sha256': sha(guest), 'module_sha256': sha(module),
                'commands': [], 'scope': 'Long-mode CPL0 CR8/APIC.TPR:272 independent legal transfers,63 primary reserved-bit cases. Known QEMU masking disagreements retained, not full equivalence.'}
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
    assert report['cases'] == 335 and report['independent_legal_cases'] == 272 and report['reserved_primary_cases'] == 63, report
    assert result.returncode == int(any(report[name] != 0 for name in ['local_profile_failed', 'independent_legal_failed', 'commit_order_failed', 'unknown_reference_profile_failed']))
    assert sha(module) == manifest['module_sha256']
    manifest['artifact_sha256'] = {name: sha(out / name) for name in ['cases.bin', 'results.bin', 'guest.s', 'guest.elf', 'compare.rs']}
    manifest['report'] = report
    (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(report))
    return result.returncode


if __name__ == '__main__':
    raise SystemExit(main())
