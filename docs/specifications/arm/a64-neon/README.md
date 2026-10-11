# A64 AdvSIMD instruction references

The unmodified instruction-reference HTML files contain Arm-authored descriptions
from DDI 0602, release `2026-03_rel`, dated 2026-03-26. They were retrieved
on 2026-10-10 from the Stanford mirror. Copyright and proprietary-notice
links remain in each file. The canonical manual is
[Arm DDI 0602](https://developer.arm.com/documentation/ddi0602/2026-03).

The files support the encoding and pseudocode review for
`tests/suites/smir/jit/aarch64_programs.rs`. A file documents its named
instruction; the table does not imply that every signed, unsigned, or
rounding variant has been retained here.

| File | Mirror source | SHA-256 |
|---|---|---|
| [cmeq_advsimd_reg.html](cmeq_advsimd_reg.html) | [CMEQ (register) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/cmeq_advsimd_reg.html) | `7d066a2834c26549f42fdfe0bc0699805dfab74f5570bf72a2b269f5f3e218ec` |
| [cmge_advsimd_reg.html](cmge_advsimd_reg.html) | [CMGE (register) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/cmge_advsimd_reg.html) | `be9b0b2a80771cc678891ee2a33ca8b8a533f89e9de523088d8c0563477dab16` |
| [cmgt_advsimd_reg.html](cmgt_advsimd_reg.html) | [CMGT (register) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/cmgt_advsimd_reg.html) | `3d4e2e014ca26af0b9bb3b025af3c241b9c2e5308e83e20887c5b4157f619ecf` |
| [cmtst_advsimd.html](cmtst_advsimd.html) | [CMTST -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/cmtst_advsimd.html) | `ea5d3c216ff940187a2dafb2a5e8cefa7664052b9db59ffc3406f980505e7161` |
| [dup_advsimd_elt.html](dup_advsimd_elt.html) | [DUP (element) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/dup_advsimd_elt.html) | `f6092a0d1fa5e4a4e1930e6f2bbc46c7957c0f73b8c5682240fba96858848ab4` |
| [dup_advsimd_gen.html](dup_advsimd_gen.html) | [DUP (general) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/dup_advsimd_gen.html) | `204a18607143c9793d8907eea2a0fc682216f88cb787e269a44862dcc6603e2a` |
| [ins_advsimd_elt.html](ins_advsimd_elt.html) | [INS (element) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/ins_advsimd_elt.html) | `0b7e84d3988289da4a0a6a7cc0f2bdc46acc483145b4386ea6b31e797d2b460c` |
| [ins_advsimd_gen.html](ins_advsimd_gen.html) | [INS (general) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/ins_advsimd_gen.html) | `0b12cf8b7bb8fd06d25464eb634d3ebe2562a6eaf07e5e7a53f4d70a24b5cebd` |
| [mla_advsimd_vec.html](mla_advsimd_vec.html) | [MLA (vector) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/mla_advsimd_vec.html) | `639c37cb95b661c09cf01daa2053d58a42b73a99af39c965a6959114b6e52e0b` |
| [saba_advsimd.html](saba_advsimd.html) | [SABA -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/saba_advsimd.html) | `277813c326c305f465bb1bed23e522f0316d76c8f25f0ee49124884696f05d57` |
| [sabd_advsimd.html](sabd_advsimd.html) | [SABD -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sabd_advsimd.html) | `b610651cbc22cc6bc3703c85b832ce4b626c08f862735766c0830738884c6db0` |
| [shadd_advsimd.html](shadd_advsimd.html) | [SHADD -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/shadd_advsimd.html) | `a5e5e40877d9eee9e001a57c5b5bd0c18356a0b7b45e193c72c80e26c575c551` |
| [shl_advsimd.html](shl_advsimd.html) | [SHL -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/shl_advsimd.html) | `6474558a9e0815f1ddd5cc92e7f11074905bc9aabeda673960ebf800d45c0be1` |
| [shsub_advsimd.html](shsub_advsimd.html) | [SHSUB -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/shsub_advsimd.html) | `164050a519091f03c72d63cb3a66bfd740ffb8f087cf1af1bff0ea37cc07d126` |
| [smax_advsimd.html](smax_advsimd.html) | [SMAX -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/smax_advsimd.html) | `9f92496a882b70b2882ef025898d92bdcd5f4602c6c9c74609b3fb965dbd1592` |
| [smov_advsimd.html](smov_advsimd.html) | [SMOV -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/smov_advsimd.html) | `eb9bbd6109da99574393743d91fd010973a1fbab654c5fccf72a87301f228b44` |
| [sshr_advsimd.html](sshr_advsimd.html) | [SSHR -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sshr_advsimd.html) | `755dcca71ad64f29539fafc361a07ad03fde68c836fef734d3f6e5826184b932` |
| [ssra_advsimd.html](ssra_advsimd.html) | [SSRA -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/ssra_advsimd.html) | `9aba69e43ead300fb392ef5a08fdab4450f1dfc1af3b32ac2238e8ebf3dcff6f` |
| [umov_advsimd.html](umov_advsimd.html) | [UMOV -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/umov_advsimd.html) | `ec98a4b5b94c6a621bca083ae4b41b43d0c6e3c123ca85019889e1666ac23f7b` |
| [sshl_advsimd.html](sshl_advsimd.html) | [SSHL -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sshl_advsimd.html) | `6dd6b2f74ab1a925ad13af653f52285efce6c820f48c1c7c556b32502823fa8f` |
| [ushl_advsimd.html](ushl_advsimd.html) | [USHL -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/ushl_advsimd.html) | `dbec96bc82be14285e5191fade56f17952f69dee75cc2ee3abcc7eb15dfccde1` |
| [srshl_advsimd.html](srshl_advsimd.html) | [SRSHL -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/srshl_advsimd.html) | `ba992172465a8a0d9ed2c8622d83805d1471677d3b318ee1e4e2c86283c4bb2b` |
| [urshl_advsimd.html](urshl_advsimd.html) | [URSHL -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/urshl_advsimd.html) | `e8e34a5a4c9914285e07de4fef743c3f9cb89044214d813fdcb711336e819169` |
| [srshr_advsimd.html](srshr_advsimd.html) | [SRSHR -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/srshr_advsimd.html) | `a358dbdafaa57da055a109db06465cb35928be281e68a175ac451f7f5458db9d` |
| [urshr_advsimd.html](urshr_advsimd.html) | [URSHR -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/urshr_advsimd.html) | `87449698b22f8782a3149816d20aa29f2d42658fd6b52456fe16284cade926da` |
| [srsra_advsimd.html](srsra_advsimd.html) | [SRSRA -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/srsra_advsimd.html) | `50a9152b43cc3cc5b25bea0e068db963e9f19d8a66ede13bfde30e13fffff6fb` |
| [ursra_advsimd.html](ursra_advsimd.html) | [URSRA -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/ursra_advsimd.html) | `3c9d54e7ce91faf09c7a196e46bb0102a628414bfbb7f66da213489bf7ac91ef` |
| [smaxp_advsimd.html](smaxp_advsimd.html) | [SMAXP -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/smaxp_advsimd.html) | `349c35952667b17c838abf234f36a57aff1ee2cfde833967c1cc64eda15887e5` |
| [umaxp_advsimd.html](umaxp_advsimd.html) | [UMAXP -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/umaxp_advsimd.html) | `d19a409bd733135f8b8864acd6db8f93d94d14306e5381faa681c67b97a3f011` |
| [sminp_advsimd.html](sminp_advsimd.html) | [SMINP -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sminp_advsimd.html) | `0b1ed93df08bbc2a76a4dafe662b38c7854578bf539f90471e7f790b22a9bf53` |
| [uminp_advsimd.html](uminp_advsimd.html) | [UMINP -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/uminp_advsimd.html) | `c1aa31ebaf23c585c56466b6222c3f609869b714b74945dde4d7516cd968e3fe` |
| [addp_advsimd_vec.html](addp_advsimd_vec.html) | [ADDP -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/addp_advsimd_vec.html) | `98882344db9323e7b7f11d3f79e81a3a32b41340a62b5717b406cd3b00b4434b` |
| [bic_advsimd_reg.html](bic_advsimd_reg.html) | [BIC -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/bic_advsimd_reg.html) | `473496ed142533327dda2ab70379e27b1ffc6dc20b6e9246668948ce33fd0c09` |
| [orn_advsimd.html](orn_advsimd.html) | [ORN -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/orn_advsimd.html) | `cf5ed67ca5f2dd865f0dcb7dcfdb74dd3bdc592b18180684dfb21cdafc9174ae` |

The following Arm-authored system-register reference was retrieved on
2026-10-10 from the Lund mirror. Its embedded revision is
`26/03/2024 09:49; 67c0ae5282a7629ba0ea0ba7267b43cd4f7939f6`, independent
of the instruction references above. It supports the CPACR_EL1.FPEN cache and
precise-trap regression tests in
`src/isa/arm/aarch64/cpu/tests/jit_permissions.rs`.

| File | Mirror source | SHA-256 |
|---|---|---|
| [AArch64-cpacr_el1.html](AArch64-cpacr_el1.html) | [CPACR_EL1](https://df.lth.se/~getz/ARM/SysReg/AArch64-cpacr_el1.html) | `ab9637f7c8c373faa83a5680f3444d199d8882dcc46d12f076560928bf85b456` |

## Integer width transitions

The following 44 Arm-authored instruction descriptions were retrieved on
2026-10-11 from the Stanford mirror. Their embedded revision is
`2026-03_rel 2026-03-26 20:48:11`. Each HTML file is retained unchanged.
These descriptions cover long arithmetic, doubling saturation, pairwise long
operations, high-half arithmetic, extraction, and immediate shifts. Scalar
forms and the upper-half `2` forms are included where the architecture defines
them. Encodings and operations are exercised by the registered
`aarch64_smir_programs` target.

| File | Mirror source | SHA-256 |
|---|---|---|
| [saddl_advsimd.html](saddl_advsimd.html) | [SADDL, SADDL2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/saddl_advsimd.html) | `b5cdaaf73162975a1534a6d49aaca2640be1f5d19d22412b1d85348671643121` |
| [uaddl_advsimd.html](uaddl_advsimd.html) | [UADDL, UADDL2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/uaddl_advsimd.html) | `15aacb6182304f99d85d085cf1d837527306757e81fbe9d6898b6a9d9f5ebec1` |
| [saddw_advsimd.html](saddw_advsimd.html) | [SADDW, SADDW2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/saddw_advsimd.html) | `8e7f4c34e50a4daffa1efac18f0e7591a2ed272aa22c0b37d83f3cdbcfb75af1` |
| [uaddw_advsimd.html](uaddw_advsimd.html) | [UADDW, UADDW2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/uaddw_advsimd.html) | `e3e7b7bead3be2a81f3073f4c9c466f0b77e22b52b5383ed565d6dc42f5cc0d7` |
| [ssubl_advsimd.html](ssubl_advsimd.html) | [SSUBL, SSUBL2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/ssubl_advsimd.html) | `87816f3171bd956e04c43e3b83612b7e1747cd51af1ee53bcfdeae2364c30177` |
| [usubl_advsimd.html](usubl_advsimd.html) | [USUBL, USUBL2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/usubl_advsimd.html) | `5de571ebf9cb5ac89a901b67d58f3f600d9570b7d265fce10835115bd591fc9d` |
| [ssubw_advsimd.html](ssubw_advsimd.html) | [SSUBW, SSUBW2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/ssubw_advsimd.html) | `c8229532662155c261290cc02f28e365b82068d605ec06fbd3733a97c035616f` |
| [usubw_advsimd.html](usubw_advsimd.html) | [USUBW, USUBW2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/usubw_advsimd.html) | `d326684d0efacf57305f38db5b7452408763ff570c78f472273fa81fd4b38ce7` |
| [sabdl_advsimd.html](sabdl_advsimd.html) | [SABDL, SABDL2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sabdl_advsimd.html) | `24fcda9d6032c4c5e7300abe1f6fead836e514d5f3bf13f73afc6677224282b7` |
| [uabdl_advsimd.html](uabdl_advsimd.html) | [UABDL, UABDL2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/uabdl_advsimd.html) | `0b0795ae7b24f6d94ee8428b91c4c414b18fc7616773c9c9ed1adfeabe05e1df` |
| [sabal_advsimd.html](sabal_advsimd.html) | [SABAL, SABAL2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sabal_advsimd.html) | `d737554ac07e0867306f6585632550b93200362ab2f544c1bce718ea40d76d95` |
| [uabal_advsimd.html](uabal_advsimd.html) | [UABAL, UABAL2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/uabal_advsimd.html) | `7946be785cf1d19c9154b3c93ad67c2e3fae0a41a232e6236e2680976578dfb9` |
| [smull_advsimd_vec.html](smull_advsimd_vec.html) | [SMULL, SMULL2 (vector) - A64](https://www.scs.stanford.edu/~zyedidia/arm64/smull_advsimd_vec.html) | `68544c9975b54f1b5379e85ef5b99b641a451ac04e1ed067b5fa7830e7451691` |
| [umull_advsimd_vec.html](umull_advsimd_vec.html) | [UMULL, UMULL2 (vector) - A64](https://www.scs.stanford.edu/~zyedidia/arm64/umull_advsimd_vec.html) | `33ce424f03e1f98bebbebdd64e141fccec07048c054f9ae715f295d14ab886c0` |
| [smlal_advsimd_vec.html](smlal_advsimd_vec.html) | [SMLAL, SMLAL2 (vector) - A64](https://www.scs.stanford.edu/~zyedidia/arm64/smlal_advsimd_vec.html) | `75af0195e4bb526f1b2c3de9c58be89b0ae11b5993a3268b2d84a5bd24911364` |
| [umlal_advsimd_vec.html](umlal_advsimd_vec.html) | [UMLAL, UMLAL2 (vector) - A64](https://www.scs.stanford.edu/~zyedidia/arm64/umlal_advsimd_vec.html) | `f7b38117842544db764f0ed7d3e7231ea546c06d705897522e55755e946f1651` |
| [smlsl_advsimd_vec.html](smlsl_advsimd_vec.html) | [SMLSL, SMLSL2 (vector) - A64](https://www.scs.stanford.edu/~zyedidia/arm64/smlsl_advsimd_vec.html) | `598426e5ea5037c480f892fd3bf2fd91330675233c461444a6955f62d8fec74b` |
| [umlsl_advsimd_vec.html](umlsl_advsimd_vec.html) | [UMLSL, UMLSL2 (vector) - A64](https://www.scs.stanford.edu/~zyedidia/arm64/umlsl_advsimd_vec.html) | `d213a2d8c2441a75fad424d5a56030a70a1a82fd5a845304b74a7f88a419c862` |
| [sqdmull_advsimd_vec.html](sqdmull_advsimd_vec.html) | [SQDMULL, SQDMULL2 (vector) - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqdmull_advsimd_vec.html) | `9b0e850892577d563a2fc72f2d92d4e6d8d59b451c88febdc973d7d71778e620` |
| [sqdmlal_advsimd_vec.html](sqdmlal_advsimd_vec.html) | [SQDMLAL, SQDMLAL2 (vector) - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqdmlal_advsimd_vec.html) | `94c39403c7c9e64aea483442b7ad6532e7a5d5373111f16cb9dd3e32a4d6cb06` |
| [sqdmlsl_advsimd_vec.html](sqdmlsl_advsimd_vec.html) | [SQDMLSL, SQDMLSL2 (vector) - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqdmlsl_advsimd_vec.html) | `e24f46258a99c997d9e5e218e892a63226f3113ccf961b7881c0905eebe4b062` |
| [sshll_advsimd.html](sshll_advsimd.html) | [SSHLL, SSHLL2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sshll_advsimd.html) | `2ff521e9d7b5b7c4f4e3a2fcc4ae6fa9321c30c977899ccabbb4aff77a55d413` |
| [ushll_advsimd.html](ushll_advsimd.html) | [USHLL, USHLL2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/ushll_advsimd.html) | `886809279f4867310f16782e920572fb874aad0b1ef3edab1575db2d1450dc2e` |
| [shll_advsimd.html](shll_advsimd.html) | [SHLL, SHLL2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/shll_advsimd.html) | `f651759747e0a6390bd89323fc30391c1b39aff18f7fe55c73f6e73e78cc1884` |
| [addhn_advsimd.html](addhn_advsimd.html) | [ADDHN, ADDHN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/addhn_advsimd.html) | `363438132f51376fbfdfe9fd51e96535da835d80db9b7d51df3e8648f83907d4` |
| [raddhn_advsimd.html](raddhn_advsimd.html) | [RADDHN, RADDHN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/raddhn_advsimd.html) | `2dbeab67fd10fc570d78d2d4a8ada88693b454e8645df1b27b9647a26b1c0ff5` |
| [subhn_advsimd.html](subhn_advsimd.html) | [SUBHN, SUBHN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/subhn_advsimd.html) | `00fc24efb23d3c4db00302f48ea386b997a1c7936f181a4fefcf30f38fdfefca` |
| [rsubhn_advsimd.html](rsubhn_advsimd.html) | [RSUBHN, RSUBHN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/rsubhn_advsimd.html) | `95c9c557c1c7eeb720ce5a67d764645d8baa49e3d91e4b315a016c54f2a75987` |
| [xtn_advsimd.html](xtn_advsimd.html) | [XTN, XTN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/xtn_advsimd.html) | `dd9ddfaf77e27ed3136eda1c4190f3b4eb828e8276adac6d44d6d974faf13bf4` |
| [sqxtn_advsimd.html](sqxtn_advsimd.html) | [SQXTN, SQXTN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqxtn_advsimd.html) | `5eac05ae9024479b898cbd86d3a244056d8fc1904606cae8c198cedafef1d357` |
| [uqxtn_advsimd.html](uqxtn_advsimd.html) | [UQXTN, UQXTN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/uqxtn_advsimd.html) | `e0d0c4358063a7b2078ddcc037d2c62fb522a9dd2a61288f4128f875716c324c` |
| [sqxtun_advsimd.html](sqxtun_advsimd.html) | [SQXTUN, SQXTUN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqxtun_advsimd.html) | `bc294e85017259843280637f064731bfd84e2fca117dffef49ca12d4acc4b7cb` |
| [shrn_advsimd.html](shrn_advsimd.html) | [SHRN, SHRN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/shrn_advsimd.html) | `577012ce845dfb77dad6a9a616cea127a88a0f62f3d564fa988fd090c1315159` |
| [rshrn_advsimd.html](rshrn_advsimd.html) | [RSHRN, RSHRN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/rshrn_advsimd.html) | `5adb564da54cccd93b817bde3357bf9f49d8204359442f940b273d24cde91114` |
| [sqshrn_advsimd.html](sqshrn_advsimd.html) | [SQSHRN, SQSHRN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqshrn_advsimd.html) | `e03dc10ae0b35f8ca3296297fa47cf58d0416c5719a9f0a081d38ed0f71cd4ac` |
| [uqshrn_advsimd.html](uqshrn_advsimd.html) | [UQSHRN, UQSHRN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/uqshrn_advsimd.html) | `1be1dfb1be2b33b9c491f789d16557a799ce8a2fedaa1a7a30093d525e471cce` |
| [sqrshrn_advsimd.html](sqrshrn_advsimd.html) | [SQRSHRN, SQRSHRN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqrshrn_advsimd.html) | `47ef02fcbf599ee0bcc90675e29aca548a1e17a784671084e8f84bc066e15111` |
| [uqrshrn_advsimd.html](uqrshrn_advsimd.html) | [UQRSHRN, UQRSHRN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/uqrshrn_advsimd.html) | `afaf10b0ffe648b8ae67cc8aee02c25f220c52427541fa08b1abb6753f5b8bdb` |
| [sqshrun_advsimd.html](sqshrun_advsimd.html) | [SQSHRUN, SQSHRUN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqshrun_advsimd.html) | `b19997adc7b7d5c8cf8f52be1f3f8845f32d7f0abac2f88022c2d8e8310f4cb5` |
| [sqrshrun_advsimd.html](sqrshrun_advsimd.html) | [SQRSHRUN, SQRSHRUN2 - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqrshrun_advsimd.html) | `32a1a1efae74b640b721962c7e0974bb1e5b199268fcefbbd53da9ebfa241e56` |
| [saddlp_advsimd.html](saddlp_advsimd.html) | [SADDLP - A64](https://www.scs.stanford.edu/~zyedidia/arm64/saddlp_advsimd.html) | `5b87dc4d974e64a5fc4c89e9742e39d2dbcb61e424118bde6bee783001dc9075` |
| [uaddlp_advsimd.html](uaddlp_advsimd.html) | [UADDLP - A64](https://www.scs.stanford.edu/~zyedidia/arm64/uaddlp_advsimd.html) | `68c56d750d019a6458f7074bf4105e794452a2af73cf4e954b42e9635befc2b2` |
| [sadalp_advsimd.html](sadalp_advsimd.html) | [SADALP - A64](https://www.scs.stanford.edu/~zyedidia/arm64/sadalp_advsimd.html) | `1e84e96e37d2640fd2ca51faf62a11e7b7b60375642cc16d210f3f91a87e803c` |
| [uadalp_advsimd.html](uadalp_advsimd.html) | [UADALP - A64](https://www.scs.stanford.edu/~zyedidia/arm64/uadalp_advsimd.html) | `588a7ffd6a3e8706ad1b8659d893653ca135b5bd0b82407c168b7dcd3e6e9987` |

## Same-width integer saturation

The following Arm-authored `2026-03_rel` files were retrieved on 2026-10-11.
They cover scalar/vector saturation, low-byte signed register shifts, immediate
shifts and indexed doubling multiply high. These forms require FEAT_AdvSIMD.
FEAT_RDM accumulating multiply forms are outside this table.

| File | Primary-source mirror | SHA-256 |
|---|---|---|
| [sqadd_advsimd.html](sqadd_advsimd.html) | [SQADD -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqadd_advsimd.html) | `5b03f6ea45bca8f4179c383e1596477b244a43baf77b816bcba4b2a52c2a2099` |
| [uqadd_advsimd.html](uqadd_advsimd.html) | [UQADD -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/uqadd_advsimd.html) | `26fed1668d6ecbd8688344bf71fab18e13cb0f7d0070c4a8284faca0ea71c1f9` |
| [sqsub_advsimd.html](sqsub_advsimd.html) | [SQSUB -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqsub_advsimd.html) | `aed29288538d4bede7d4c591135bfc4a62e31561454b38c008e36508d416f37c` |
| [uqsub_advsimd.html](uqsub_advsimd.html) | [UQSUB -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/uqsub_advsimd.html) | `a6c50504647826980d96fe8445fbfad9c737f111b6bf0f3274dac4bad96766a1` |
| [suqadd_advsimd.html](suqadd_advsimd.html) | [SUQADD -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/suqadd_advsimd.html) | `e746aa779adb5df8532f1d20d5f060df08e4a1c381192ea0b6fc9d4f56b5b902` |
| [usqadd_advsimd.html](usqadd_advsimd.html) | [USQADD -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/usqadd_advsimd.html) | `ce1a66c0664fc7dea4863e37bd083a3168d676e1ed7e778172a846c9df2ea633` |
| [sqabs_advsimd.html](sqabs_advsimd.html) | [SQABS -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqabs_advsimd.html) | `ca9e69fbbc6bc197cad1d36b62a152ae0d1db8fcfad454f2bebcfe29f5c7fe3f` |
| [sqneg_advsimd.html](sqneg_advsimd.html) | [SQNEG -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqneg_advsimd.html) | `fafbbcb1c321868143af42298dfbdebe5bc327c100a95d05a88ce48eb32ce904` |
| [sqshl_advsimd_reg.html](sqshl_advsimd_reg.html) | [SQSHL (register) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqshl_advsimd_reg.html) | `e8d06abb62b90e4a560c889eaebc9df8fe5a9ec068314a51f7c2187363c31cc0` |
| [uqshl_advsimd_reg.html](uqshl_advsimd_reg.html) | [UQSHL (register) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/uqshl_advsimd_reg.html) | `0bc2055efd9e380fb4437249077983e3a5797b40c6f4399d190dfd3c89ee11a1` |
| [sqrshl_advsimd.html](sqrshl_advsimd.html) | [SQRSHL -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqrshl_advsimd.html) | `c2d53b399cccf1b90a2afdfbe8d5869c6ffa0de540e47bd19340e358619fdcec` |
| [uqrshl_advsimd.html](uqrshl_advsimd.html) | [UQRSHL -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/uqrshl_advsimd.html) | `5314852083d05c748b8740d582e8d788d66582b99065ba66586b9098863f546b` |
| [sqshl_advsimd_imm.html](sqshl_advsimd_imm.html) | [SQSHL (immediate) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqshl_advsimd_imm.html) | `f924d559f2b521e78c09e1edeb31611d12c7b9042259d0da969706d039700bbb` |
| [uqshl_advsimd_imm.html](uqshl_advsimd_imm.html) | [UQSHL (immediate) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/uqshl_advsimd_imm.html) | `a6948f571d4f0d4f4a850ae169e43d5a74ad8816e1ec5068af155be3f31271e8` |
| [sqshlu_advsimd.html](sqshlu_advsimd.html) | [SQSHLU -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqshlu_advsimd.html) | `c9618bfee4a08b4103c45f3773004f7e2e3261fbe76bb49d2b930b856e25541b` |
| [sqdmulh_advsimd_vec.html](sqdmulh_advsimd_vec.html) | [SQDMULH (vector) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqdmulh_advsimd_vec.html) | `7219e82971ce5452607784539dc6260e2cff1c7dad5a1fb571eb29e0dc37e673` |
| [sqrdmulh_advsimd_vec.html](sqrdmulh_advsimd_vec.html) | [SQRDMULH (vector) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqrdmulh_advsimd_vec.html) | `469b9cfdd7284e6af058820b82b8f00543ec1bdcdcbea0b04cb10ccdfc49fa2b` |
| [sqdmulh_advsimd_elt.html](sqdmulh_advsimd_elt.html) | [SQDMULH (by element) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqdmulh_advsimd_elt.html) | `3942d16aeb7e1f38f0e318be357919f8cdbd9dbbf6c1fe2de13c685a01867a06` |
| [sqrdmulh_advsimd_elt.html](sqrdmulh_advsimd_elt.html) | [SQRDMULH (by element) -- A64](https://www.scs.stanford.edu/~zyedidia/arm64/sqrdmulh_advsimd_elt.html) | `a424a4c632004d963bc2c914d8517a93c817d93aa2f24ab42d6174053bb8d278` |

## Scalar integer and comparison-zero batch

The following unmodified Arm-authored descriptions, release `2026-03_rel`,
were retrieved on 2026-10-11. They cover scalar integer forms, comparisons
against zero, pairwise addition, and immediate shift accumulation. The earlier
register-comparison and shift descriptions also contain their scalar forms.

| File | Mirror source | SHA-256 |
|---|---|---|
| [add_advsimd.html](add_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/add_advsimd.html) | `06215add796d3f385e1e660fa9c4e4fe99a3fb9e12bbc3c35b435923e67f3229` |
| [sub_advsimd.html](sub_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/sub_advsimd.html) | `e9860eb2850fc3c508e2db3f1763a961d6d22fb2f761447e0cbcced1a0335ae5` |
| [abs_advsimd.html](abs_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/abs_advsimd.html) | `29171d3f81df3719c1c7996db9519da038ecb0bc0585d35b9149de2b159b23c6` |
| [neg_advsimd.html](neg_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/neg_advsimd.html) | `3df8274dec8a1e38c43950571d1d7db3ae54072c7b7c99fc0061dc86fe64114b` |
| [cmgt_advsimd_zero.html](cmgt_advsimd_zero.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/cmgt_advsimd_zero.html) | `8edfb856510414376d44d5ba0f25e15e7ff9b686ba992f3d7e4b4285ad1da80c` |
| [cmge_advsimd_zero.html](cmge_advsimd_zero.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/cmge_advsimd_zero.html) | `5d337532efb5b93d9236471cdf6c1876f2b1ca1724b5b6053ee6a0788ea3bbc1` |
| [cmeq_advsimd_zero.html](cmeq_advsimd_zero.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/cmeq_advsimd_zero.html) | `72ea25c5576f6b07fdc970173fe41f197e70dd9052add6fa11bc875c2640911f` |
| [cmle_advsimd.html](cmle_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/cmle_advsimd.html) | `b2424600dc296f0a11f53e4e1d5ceb1b410f0aaed98116f0b835906baeb780c4` |
| [cmlt_advsimd.html](cmlt_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/cmlt_advsimd.html) | `d931fd74cceb3b471fe82033ab58df7d3188bc8ba85daa1e5a4ebba85e71d8a7` |
| [cmhi_advsimd.html](cmhi_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/cmhi_advsimd.html) | `dc687a17a536c3e712cf92b1ca852e5ef2e521e298d989d684ab99582c9d639c` |
| [cmhs_advsimd.html](cmhs_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/cmhs_advsimd.html) | `46b3d1e256f854e993a39268cde0d8686919d316a34b361c5d2959ed14104852` |
| [addp_advsimd_pair.html](addp_advsimd_pair.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/addp_advsimd_pair.html) | `f8f2b8dd5497c6243d45ae17dcba11b5780ebb93ad6b9b3cf2d48dd90ed9cfd0` |
| [ushr_advsimd.html](ushr_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/ushr_advsimd.html) | `7bd43ba24f47630cbab81b6aadae1ae39182d504e6587eba90bad4fb3b99f552` |
| [ssra_advsimd.html](ssra_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/ssra_advsimd.html) | `9aba69e43ead300fb392ef5a08fdab4450f1dfc1af3b32ac2238e8ebf3dcff6f` |
| [usra_advsimd.html](usra_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/usra_advsimd.html) | `def88a7283d0519e7d1e64822e2bc049165b42b61ce225c9f4b1a41fd6f73851` |
| [srsra_advsimd.html](srsra_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/srsra_advsimd.html) | `50a9152b43cc3cc5b25bea0e068db963e9f19d8a66ede13bfde30e13fffff6fb` |
| [ursra_advsimd.html](ursra_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/ursra_advsimd.html) | `3c9d54e7ce91faf09c7a196e46bb0102a628414bfbb7f66da213489bf7ac91ef` |
| [addv_advsimd.html](addv_advsimd.html) | [Arm instruction description](https://www.scs.stanford.edu/~zyedidia/arm64/addv_advsimd.html) | `dd9f1ea98dc7a179b04d5ef305314844c33658a4cfa7fa173bc01ae5e597ce36` |
