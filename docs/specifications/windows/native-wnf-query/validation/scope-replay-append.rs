
#[test]
fn native_wnf_query_replays_session_user_machine_scope_and_typed_denial_controls() {
    let control:serde_json::Value=serde_json::from_str(include_str!("../../../../../../../docs/specifications/windows/native-wnf-query/controls/wnf-private-scope-order.json")).unwrap();
    assert_eq!(control["rows"].as_array().unwrap().len(),168);
    for (arch,laa,profile) in [(WinArch::Arm64,false,"arm64"),(WinArch::X64,false,"x64"),(WinArch::X86,false,"x86"),(WinArch::X86,true,"x86-laa")] {
        let (mut process,mut thread,b,_)=setup(arch,laa,false,profile);let p=process.state_mut();
        let mut records=Vec::new();
        for scope in [0u64,1,2,4] {for denied in [false,true] {
            let name=0x41C6_4E6D_A3BC_0074 ^ (0x31 | (scope<<6) | (u64::from(denied)<<11));
            let record=if denied {Record::Status(STATUS_ACCESS_DENIED)}else{Record::Data(Data{stamp:0,bytes:Arc::from([])})};
            records.push((Definition{name,maximum:64,type_id:Some(guid().try_into().unwrap())},record));
        }}
        p.objects.select_wnf_catalog(crate::user::windows::wnf::tests::empty_subject_catalog(false).with_queries(Queries::selected(records,0).unwrap()));
        for (i,row) in control["rows"].as_array().unwrap().iter().enumerate() {
            reset(p,b);let scope=row["scope"].as_u64().unwrap();let denied=row["denied"].as_bool().unwrap();
            let name=0x41C6_4E6D_A3BC_0074 ^ (0x31 | (scope<<6) | (u64::from(denied)<<11));p.space.w64(b,name).unwrap();
            let type_ptr=match row["type"].as_str().unwrap() {"null"=>0,"match"=>{p.space.wr(b+64,&guid()).unwrap();b+64},"wrong"=>{p.space.wr(b+64,&hex("781b347a2c2b4e4d8182838485868789")).unwrap();b+64},_=>unreachable!()};
            let explicit=match row["explicit"].as_str().unwrap() {
                "null"=>0,"current-process"=>arch.ptr(u64::MAX),"current-thread"=>arch.ptr(u64::MAX-1),
                "user-sid"=>{p.space.wr(b+128,&p.objects.primary_token_subject().unwrap().user).unwrap();b+128},
                "world-sid"=>{p.space.wr(b+128,&hex("010100000000000100000000")).unwrap();b+128},
                "session-id"=>{p.space.w32(b+128,0).unwrap();b+128},"pid"=>{p.space.w32(b+128,p.pid).unwrap();b+128},_=>unreachable!()};
            p.space.w32(b+768,64).unwrap();let args=[b,type_ptr,explicit,b+256,b+512,b+768];
            assert_eq!(invoke(p,&mut thread,&args),row["status"].as_u64().unwrap() as u32,"{profile} scope control{i}/{row}");
            assert_eq!(p.space.u32(b+256).unwrap(),row["stamp"].as_u64().unwrap() as u32);
            assert_eq!(p.space.u32(b+768).unwrap(),row["length"].as_u64().unwrap() as u32);
            assert_eq!(p.space.bytes(b+512,64).unwrap(),hex(row["bytes"].as_str().unwrap()));
        }
    }
}
