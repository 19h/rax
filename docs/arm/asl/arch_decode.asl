__decode A64
    // A64
    case (29 +: 3, 25 +: 4, 0 +: 25) of
        when (_, '0000', _) =>
            // reserved
            case (29 +: 3, 25 +: 4, 16 +: 9, 0 +: 16) of
                when ('000', _, '000000000', _) => // perm_undef
                    __field imm16 0 +: 16
                    case () of
                        when () => __encoding aarch64_udf // UDF_only_perm_undef
                when ('000', _, !'000000000', _) => __UNPREDICTABLE
                when (!'000', _, '000000000', _) => __UNPREDICTABLE
        when (_, '0010', _) =>
            // sve
            case (29 +: 3, 25 +: 4, 23 +: 2, 22 +: 1, 17 +: 5, 16 +: 1, 10 +: 6, 0 +: 10) of
                when ('000', _, '0x', _, '0xxxx', _, 'x1xxxx', _) =>
                    // sve_int_muladd_pred
                    case (24 +: 8, 22 +: 2, 21 +: 1, 16 +: 5, 15 +: 1, 14 +: 1, 0 +: 14) of
                        when (_, _, _, _, '0', _, _) => // sve_int_mlas_vvv_pred
                            __field size 22 +: 2
                            __field Zm 16 +: 5
                            __field op 13 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zda 0 +: 5
                            case (op) of
                                when ('0') => __encoding MLA_Z.P.ZZZ__ // mla_z_p_zzz_
                                when ('1') => __encoding MLS_Z.P.ZZZ__ // mls_z_p_zzz_
                        when (_, _, _, _, '1', _, _) => // sve_int_mladdsub_vvv_pred
                            __field size 22 +: 2
                            __field Zm 16 +: 5
                            __field op 13 +: 1
                            __field Pg 10 +: 3
                            __field Za 5 +: 5
                            __field Zdn 0 +: 5
                            case (op) of
                                when ('0') => __encoding MAD_Z.P.ZZZ__ // mad_z_p_zzz_
                                when ('1') => __encoding MSB_Z.P.ZZZ__ // msb_z_p_zzz_
                when ('000', _, '0x', _, '0xxxx', _, '000xxx', _) =>
                    // sve_int_pred_bin
                    case (24 +: 8, 22 +: 2, 21 +: 1, 18 +: 3, 16 +: 2, 13 +: 3, 0 +: 13) of
                        when (_, _, _, '00x', _, _, _) => // sve_int_bin_pred_arit_0
                            __field size 22 +: 2
                            __field opc 16 +: 3
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case (opc) of
                                when ('000') => __encoding ADD_Z.P.ZZ__ // add_z_p_zz_
                                when ('001') => __encoding SUB_Z.P.ZZ__ // sub_z_p_zz_
                                when ('010') => __UNALLOCATED
                                when ('011') => __encoding SUBR_Z.P.ZZ__ // subr_z_p_zz_
                                when ('1xx') => __UNALLOCATED
                        when (_, _, _, '01x', _, _, _) => // sve_int_bin_pred_arit_1
                            __field size 22 +: 2
                            __field opc 17 +: 2
                            __field U 16 +: 1
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case (opc, U) of
                                when ('00', '0') => __encoding SMAX_Z.P.ZZ__ // smax_z_p_zz_
                                when ('00', '1') => __encoding UMAX_Z.P.ZZ__ // umax_z_p_zz_
                                when ('01', '0') => __encoding SMIN_Z.P.ZZ__ // smin_z_p_zz_
                                when ('01', '1') => __encoding UMIN_Z.P.ZZ__ // umin_z_p_zz_
                                when ('10', '0') => __encoding SABD_Z.P.ZZ__ // sabd_z_p_zz_
                                when ('10', '1') => __encoding UABD_Z.P.ZZ__ // uabd_z_p_zz_
                                when ('11', _) => __UNALLOCATED
                        when (_, _, _, '100', _, _, _) => // sve_int_bin_pred_arit_2
                            __field size 22 +: 2
                            __field H 17 +: 1
                            __field U 16 +: 1
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case (H, U) of
                                when ('0', '0') => __encoding MUL_Z.P.ZZ__ // mul_z_p_zz_
                                when ('0', '1') => __UNALLOCATED
                                when ('1', '0') => __encoding SMULH_Z.P.ZZ__ // smulh_z_p_zz_
                                when ('1', '1') => __encoding UMULH_Z.P.ZZ__ // umulh_z_p_zz_
                        when (_, _, _, '101', _, _, _) => // sve_int_bin_pred_div
                            __field size 22 +: 2
                            __field R 17 +: 1
                            __field U 16 +: 1
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case (R, U) of
                                when ('0', '0') => __encoding SDIV_Z.P.ZZ__ // sdiv_z_p_zz_
                                when ('0', '1') => __encoding UDIV_Z.P.ZZ__ // udiv_z_p_zz_
                                when ('1', '0') => __encoding SDIVR_Z.P.ZZ__ // sdivr_z_p_zz_
                                when ('1', '1') => __encoding UDIVR_Z.P.ZZ__ // udivr_z_p_zz_
                        when (_, _, _, '11x', _, _, _) => // sve_int_bin_pred_log
                            __field size 22 +: 2
                            __field opc 16 +: 3
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case (opc) of
                                when ('000') => __encoding ORR_Z.P.ZZ__ // orr_z_p_zz_
                                when ('001') => __encoding EOR_Z.P.ZZ__ // eor_z_p_zz_
                                when ('010') => __encoding AND_Z.P.ZZ__ // and_z_p_zz_
                                when ('011') => __encoding BIC_Z.P.ZZ__ // bic_z_p_zz_
                                when ('1xx') => __UNALLOCATED
                when ('000', _, '0x', _, '0xxxx', _, '001xxx', _) =>
                    // sve_int_pred_red
                    case (24 +: 8, 22 +: 2, 21 +: 1, 19 +: 2, 16 +: 3, 13 +: 3, 0 +: 13) of
                        when (_, _, _, '00', _, _, _) => // sve_int_reduce_0
                            __field size 22 +: 2
                            __field opc 17 +: 2
                            __field U 16 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Vd 0 +: 5
                            case (opc, U) of
                                when ('00', '0') => __encoding SADDV_R.P.Z__ // saddv_r_p_z_
                                when ('00', '1') => __encoding UADDV_R.P.Z__ // uaddv_r_p_z_
                                when ('01', _) => __UNALLOCATED
                                when ('1x', _) => __UNALLOCATED
                        when (_, _, _, '01', _, _, _) => // sve_int_reduce_1
                            __field size 22 +: 2
                            __field opc 17 +: 2
                            __field U 16 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Vd 0 +: 5
                            case (opc, U) of
                                when ('00', '0') => __encoding SMAXV_R.P.Z__ // smaxv_r_p_z_
                                when ('00', '1') => __encoding UMAXV_R.P.Z__ // umaxv_r_p_z_
                                when ('01', '0') => __encoding SMINV_R.P.Z__ // sminv_r_p_z_
                                when ('01', '1') => __encoding UMINV_R.P.Z__ // uminv_r_p_z_
                                when ('1x', _) => __UNALLOCATED
                        when (_, _, _, '10', _, _, _) => // sve_int_movprfx_pred
                            __field size 22 +: 2
                            __field opc 17 +: 2
                            __field M 16 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc) of
                                when ('00') => __encoding MOVPRFX_Z.P.Z__ // movprfx_z_p_z_
                                when ('01') => __UNALLOCATED
                                when ('1x') => __UNALLOCATED
                        when (_, _, _, '11', _, _, _) => // sve_int_reduce_2
                            __field size 22 +: 2
                            __field opc 16 +: 3
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Vd 0 +: 5
                            case (opc) of
                                when ('000') => __encoding ORV_R.P.Z__ // orv_r_p_z_
                                when ('001') => __encoding EORV_R.P.Z__ // eorv_r_p_z_
                                when ('010') => __encoding ANDV_R.P.Z__ // andv_r_p_z_
                                when ('011') => __UNALLOCATED
                                when ('1xx') => __UNALLOCATED
                when ('000', _, '0x', _, '0xxxx', _, '100xxx', _) =>
                    // sve_int_pred_shift
                    case (24 +: 8, 22 +: 2, 21 +: 1, 19 +: 2, 16 +: 3, 13 +: 3, 0 +: 13) of
                        when (_, _, _, '0x', _, _, _) => // sve_int_bin_pred_shift_0
                            __field tszh 22 +: 2
                            __field opc 18 +: 2
                            __field L 17 +: 1
                            __field U 16 +: 1
                            __field Pg 10 +: 3
                            __field tszl 8 +: 2
                            __field imm3 5 +: 3
                            __field Zdn 0 +: 5
                            case (opc, L, U) of
                                when ('00', '0', '0') => __encoding ASR_Z.P.ZI__ // asr_z_p_zi_
                                when ('00', '0', '1') => __encoding LSR_Z.P.ZI__ // lsr_z_p_zi_
                                when ('00', '1', '0') => __UNALLOCATED
                                when ('00', '1', '1') => __encoding LSL_Z.P.ZI__ // lsl_z_p_zi_
                                when ('01', '0', '0') => __encoding ASRD_Z.P.ZI__ // asrd_z_p_zi_
                                when ('01', '0', '1') => __UNALLOCATED
                                when ('01', '1', _) => __UNALLOCATED
                                when ('1x', _, _) => __UNALLOCATED
                        when (_, _, _, '10', _, _, _) => // sve_int_bin_pred_shift_1
                            __field size 22 +: 2
                            __field R 18 +: 1
                            __field L 17 +: 1
                            __field U 16 +: 1
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case (R, L, U) of
                                when (_, '1', '0') => __UNALLOCATED
                                when ('0', '0', '0') => __encoding ASR_Z.P.ZZ__ // asr_z_p_zz_
                                when ('0', '0', '1') => __encoding LSR_Z.P.ZZ__ // lsr_z_p_zz_
                                when ('0', '1', '1') => __encoding LSL_Z.P.ZZ__ // lsl_z_p_zz_
                                when ('1', '0', '0') => __encoding ASRR_Z.P.ZZ__ // asrr_z_p_zz_
                                when ('1', '0', '1') => __encoding LSRR_Z.P.ZZ__ // lsrr_z_p_zz_
                                when ('1', '1', '1') => __encoding LSLR_Z.P.ZZ__ // lslr_z_p_zz_
                        when (_, _, _, '11', _, _, _) => // sve_int_bin_pred_shift_2
                            __field size 22 +: 2
                            __field R 18 +: 1
                            __field L 17 +: 1
                            __field U 16 +: 1
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case (R, L, U) of
                                when ('0', '0', '0') => __encoding ASR_Z.P.ZW__ // asr_z_p_zw_
                                when ('0', '0', '1') => __encoding LSR_Z.P.ZW__ // lsr_z_p_zw_
                                when ('0', '1', '0') => __UNALLOCATED
                                when ('0', '1', '1') => __encoding LSL_Z.P.ZW__ // lsl_z_p_zw_
                                when ('1', _, _) => __UNALLOCATED
                when ('000', _, '0x', _, '0xxxx', _, '101xxx', _) =>
                    // sve_int_pred_un
                    case (24 +: 8, 22 +: 2, 21 +: 1, 19 +: 2, 16 +: 3, 13 +: 3, 0 +: 13) of
                        when (_, _, _, '0x', _, _, _) => __UNPREDICTABLE
                        when (_, _, _, '10', _, _, _) => // sve_int_un_pred_arit_0
                            __field size 22 +: 2
                            __field opc 16 +: 3
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc) of
                                when ('000') => __encoding SXTB_Z.P.Z__ // sxtb_z_p_z_
                                when ('001') => __encoding UXTB_Z.P.Z__ // uxtb_z_p_z_
                                when ('010') => __encoding SXTH_Z.P.Z__ // sxth_z_p_z_
                                when ('011') => __encoding UXTH_Z.P.Z__ // uxth_z_p_z_
                                when ('100') => __encoding SXTW_Z.P.Z__ // sxtw_z_p_z_
                                when ('101') => __encoding UXTW_Z.P.Z__ // uxtw_z_p_z_
                                when ('110') => __encoding ABS_Z.P.Z__ // abs_z_p_z_
                                when ('111') => __encoding NEG_Z.P.Z__ // neg_z_p_z_
                        when (_, _, _, '11', _, _, _) => // sve_int_un_pred_arit_1
                            __field size 22 +: 2
                            __field opc 16 +: 3
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc) of
                                when ('000') => __encoding CLS_Z.P.Z__ // cls_z_p_z_
                                when ('001') => __encoding CLZ_Z.P.Z__ // clz_z_p_z_
                                when ('010') => __encoding CNT_Z.P.Z__ // cnt_z_p_z_
                                when ('011') => __encoding CNOT_Z.P.Z__ // cnot_z_p_z_
                                when ('100') => __encoding FABS_Z.P.Z__ // fabs_z_p_z_
                                when ('101') => __encoding FNEG_Z.P.Z__ // fneg_z_p_z_
                                when ('110') => __encoding NOT_Z.P.Z__ // not_z_p_z_
                                when ('111') => __UNALLOCATED
                when ('000', _, '0x', _, '1xxxx', _, '000xxx', _) => // sve_int_bin_cons_arit_0
                    __field size 22 +: 2
                    __field Zm 16 +: 5
                    __field opc 10 +: 3
                    __field Zn 5 +: 5
                    __field Zd 0 +: 5
                    case (opc) of
                        when ('000') => __encoding ADD_Z.ZZ__ // add_z_zz_
                        when ('001') => __encoding SUB_Z.ZZ__ // sub_z_zz_
                        when ('01x') => __UNALLOCATED
                        when ('100') => __encoding SQADD_Z.ZZ__ // sqadd_z_zz_
                        when ('101') => __encoding UQADD_Z.ZZ__ // uqadd_z_zz_
                        when ('110') => __encoding SQSUB_Z.ZZ__ // sqsub_z_zz_
                        when ('111') => __encoding UQSUB_Z.ZZ__ // uqsub_z_zz_
                when ('000', _, '0x', _, '1xxxx', _, '001xxx', _) =>
                    // sve_int_unpred_logical
                    case (24 +: 8, 22 +: 2, 21 +: 1, 16 +: 5, 13 +: 3, 12 +: 1, 10 +: 2, 0 +: 10) of
                        when (_, _, _, _, _, '0', _, _) => __UNPREDICTABLE
                        when (_, _, _, _, _, '1', '00', _) => // sve_int_bin_cons_log
                            __field opc 22 +: 2
                            __field Zm 16 +: 5
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc) of
                                when ('00') => __encoding AND_Z.ZZ__ // and_z_zz_
                                when ('01') => __encoding ORR_Z.ZZ__ // orr_z_zz_
                                when ('10') => __encoding EOR_Z.ZZ__ // eor_z_zz_
                                when ('11') => __encoding BIC_Z.ZZ__ // bic_z_zz_
                        when (_, _, _, _, _, '1', !'00', _) => __UNPREDICTABLE
                when ('000', _, '0x', _, '1xxxx', _, '0100xx', _) =>
                    // sve_index
                    case (24 +: 8, 22 +: 2, 21 +: 1, 16 +: 5, 12 +: 4, 10 +: 2, 0 +: 10) of
                        when (_, _, _, _, _, '00', _) => // sve_int_index_ii
                            __field size 22 +: 2
                            __field imm5b 16 +: 5
                            __field imm5 5 +: 5
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding INDEX_Z.II__ // index_z_ii_
                        when (_, _, _, _, _, '01', _) => // sve_int_index_ri
                            __field size 22 +: 2
                            __field imm5 16 +: 5
                            __field Rn 5 +: 5
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding INDEX_Z.RI__ // index_z_ri_
                        when (_, _, _, _, _, '10', _) => // sve_int_index_ir
                            __field size 22 +: 2
                            __field Rm 16 +: 5
                            __field imm5 5 +: 5
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding INDEX_Z.IR__ // index_z_ir_
                        when (_, _, _, _, _, '11', _) => // sve_int_index_rr
                            __field size 22 +: 2
                            __field Rm 16 +: 5
                            __field Rn 5 +: 5
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding INDEX_Z.RR__ // index_z_rr_
                when ('000', _, '0x', _, '1xxxx', _, '0101xx', _) =>
                    // sve_alloca
                    case (24 +: 8, 23 +: 1, 22 +: 1, 21 +: 1, 16 +: 5, 12 +: 4, 11 +: 1, 0 +: 11) of
                        when (_, '0', _, _, _, _, '0', _) => // sve_int_arith_vl
                            __field op 22 +: 1
                            __field Rn 16 +: 5
                            __field imm6 5 +: 6
                            __field Rd 0 +: 5
                            case (op) of
                                when ('0') => __encoding ADDVL_R.RI__ // addvl_r_ri_
                                when ('1') => __encoding ADDPL_R.RI__ // addpl_r_ri_
                        when (_, '1', _, _, _, _, '0', _) => // sve_int_read_vl_a
                            __field op 22 +: 1
                            __field opc2 16 +: 5
                            __field imm6 5 +: 6
                            __field Rd 0 +: 5
                            case (op, opc2) of
                                when ('0', '0xxxx') => __UNALLOCATED
                                when ('0', '10xxx') => __UNALLOCATED
                                when ('0', '110xx') => __UNALLOCATED
                                when ('0', '1110x') => __UNALLOCATED
                                when ('0', '11110') => __UNALLOCATED
                                when ('0', '11111') => __encoding RDVL_R.I__ // rdvl_r_i_
                                when ('1', _) => __UNALLOCATED
                        when (_, _, _, _, _, _, '1', _) => __UNPREDICTABLE
                when ('000', _, '0x', _, '1xxxx', _, '011xxx', _) => __UNPREDICTABLE
                when ('000', _, '0x', _, '1xxxx', _, '100xxx', _) =>
                    // sve_int_unpred_shift
                    case (24 +: 8, 22 +: 2, 21 +: 1, 16 +: 5, 13 +: 3, 12 +: 1, 0 +: 12) of
                        when (_, _, _, _, _, '0', _) => // sve_int_bin_cons_shift_a
                            __field size 22 +: 2
                            __field Zm 16 +: 5
                            __field opc 10 +: 2
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc) of
                                when ('00') => __encoding ASR_Z.ZW__ // asr_z_zw_
                                when ('01') => __encoding LSR_Z.ZW__ // lsr_z_zw_
                                when ('10') => __UNALLOCATED
                                when ('11') => __encoding LSL_Z.ZW__ // lsl_z_zw_
                        when (_, _, _, _, _, '1', _) => // sve_int_bin_cons_shift_b
                            __field tszh 22 +: 2
                            __field tszl 19 +: 2
                            __field imm3 16 +: 3
                            __field opc 10 +: 2
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc) of
                                when ('00') => __encoding ASR_Z.ZI__ // asr_z_zi_
                                when ('01') => __encoding LSR_Z.ZI__ // lsr_z_zi_
                                when ('10') => __UNALLOCATED
                                when ('11') => __encoding LSL_Z.ZI__ // lsl_z_zi_
                when ('000', _, '0x', _, '1xxxx', _, '1010xx', _) => // sve_int_bin_cons_misc_0_a
                    __field opc 22 +: 2
                    __field Zm 16 +: 5
                    __field msz 10 +: 2
                    __field Zn 5 +: 5
                    __field Zd 0 +: 5
                    case (opc) of
                        when ('00') => __encoding ADR_Z.AZ_D.s32.scaled // adr_z_az_d_s32_scaled
                        when ('01') => __encoding ADR_Z.AZ_D.u32.scaled // adr_z_az_d_u32_scaled
                        when ('1x') => __encoding ADR_Z.AZ_SD.same.scaled // adr_z_az_sd_same_scaled
                when ('000', _, '0x', _, '1xxxx', _, '1011xx', _) =>
                    // sve_int_unpred_misc
                    case (24 +: 8, 22 +: 2, 21 +: 1, 16 +: 5, 12 +: 4, 10 +: 2, 0 +: 10) of
                        when (_, _, _, _, _, '0x', _) => // sve_int_bin_cons_misc_0_b
                            __field size 22 +: 2
                            __field Zm 16 +: 5
                            __field op 10 +: 1
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (op) of
                                when ('0') => __encoding FTSSEL_Z.ZZ__ // ftssel_z_zz_
                                when ('1') => __UNALLOCATED
                        when (_, _, _, _, _, '10', _) => // sve_int_bin_cons_misc_0_c
                            __field size 22 +: 2
                            __field opc 16 +: 5
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc) of
                                when ('00000') => __encoding FEXPA_Z.Z__ // fexpa_z_z_
                                when ('00001') => __UNALLOCATED
                                when ('0001x') => __UNALLOCATED
                                when ('001xx') => __UNALLOCATED
                                when ('01xxx') => __UNALLOCATED
                                when ('1xxxx') => __UNALLOCATED
                        when (_, _, _, _, _, '11', _) => // sve_int_bin_cons_misc_0_d
                            __field opc 22 +: 2
                            __field opc2 16 +: 5
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc, opc2) of
                                when ('00', '00000') => __encoding MOVPRFX_Z.Z__ // movprfx_z_z_
                                when ('00', '00001') => __UNALLOCATED
                                when ('00', '0001x') => __UNALLOCATED
                                when ('00', '001xx') => __UNALLOCATED
                                when ('00', '01xxx') => __UNALLOCATED
                                when ('00', '1xxxx') => __UNALLOCATED
                                when ('01', _) => __UNALLOCATED
                                when ('1x', _) => __UNALLOCATED
                when ('000', _, '0x', _, '1xxxx', _, '11xxxx', _) =>
                    // sve_countelt
                    case (24 +: 8, 22 +: 2, 21 +: 1, 20 +: 1, 16 +: 4, 14 +: 2, 11 +: 3, 0 +: 11) of
                        when (_, _, _, '0', _, _, '00x', _) => // sve_int_countvlv0
                            __field size 22 +: 2
                            __field imm4 16 +: 4
                            __field D 11 +: 1
                            __field U 10 +: 1
                            __field pattern 5 +: 5
                            __field Zdn 0 +: 5
                            case (size, D, U) of
                                when ('00', _, _) => __UNALLOCATED
                                when ('01', '0', '0') => __encoding SQINCH_Z.ZS__ // sqinch_z_zs_
                                when ('01', '0', '1') => __encoding UQINCH_Z.ZS__ // uqinch_z_zs_
                                when ('01', '1', '0') => __encoding SQDECH_Z.ZS__ // sqdech_z_zs_
                                when ('01', '1', '1') => __encoding UQDECH_Z.ZS__ // uqdech_z_zs_
                                when ('10', '0', '0') => __encoding SQINCW_Z.ZS__ // sqincw_z_zs_
                                when ('10', '0', '1') => __encoding UQINCW_Z.ZS__ // uqincw_z_zs_
                                when ('10', '1', '0') => __encoding SQDECW_Z.ZS__ // sqdecw_z_zs_
                                when ('10', '1', '1') => __encoding UQDECW_Z.ZS__ // uqdecw_z_zs_
                                when ('11', '0', '0') => __encoding SQINCD_Z.ZS__ // sqincd_z_zs_
                                when ('11', '0', '1') => __encoding UQINCD_Z.ZS__ // uqincd_z_zs_
                                when ('11', '1', '0') => __encoding SQDECD_Z.ZS__ // sqdecd_z_zs_
                                when ('11', '1', '1') => __encoding UQDECD_Z.ZS__ // uqdecd_z_zs_
                        when (_, _, _, '0', _, _, '100', _) => // sve_int_count
                            __field size 22 +: 2
                            __field imm4 16 +: 4
                            __field op 10 +: 1
                            __field pattern 5 +: 5
                            __field Rd 0 +: 5
                            case (size, op) of
                                when (_, '1') => __UNALLOCATED
                                when ('00', '0') => __encoding CNTB_R.S__ // cntb_r_s_
                                when ('01', '0') => __encoding CNTH_R.S__ // cnth_r_s_
                                when ('10', '0') => __encoding CNTW_R.S__ // cntw_r_s_
                                when ('11', '0') => __encoding CNTD_R.S__ // cntd_r_s_
                        when (_, _, _, '0', _, _, '101', _) => __UNPREDICTABLE
                        when (_, _, _, '1', _, _, '000', _) => // sve_int_countvlv1
                            __field size 22 +: 2
                            __field imm4 16 +: 4
                            __field D 10 +: 1
                            __field pattern 5 +: 5
                            __field Zdn 0 +: 5
                            case (size, D) of
                                when ('00', _) => __UNALLOCATED
                                when ('01', '0') => __encoding INCH_Z.ZS__ // inch_z_zs_
                                when ('01', '1') => __encoding DECH_Z.ZS__ // dech_z_zs_
                                when ('10', '0') => __encoding INCW_Z.ZS__ // incw_z_zs_
                                when ('10', '1') => __encoding DECW_Z.ZS__ // decw_z_zs_
                                when ('11', '0') => __encoding INCD_Z.ZS__ // incd_z_zs_
                                when ('11', '1') => __encoding DECD_Z.ZS__ // decd_z_zs_
                        when (_, _, _, '1', _, _, '100', _) => // sve_int_pred_pattern_a
                            __field size 22 +: 2
                            __field imm4 16 +: 4
                            __field D 10 +: 1
                            __field pattern 5 +: 5
                            __field Rdn 0 +: 5
                            case (size, D) of
                                when ('00', '0') => __encoding INCB_R.RS__ // incb_r_rs_
                                when ('00', '1') => __encoding DECB_R.RS__ // decb_r_rs_
                                when ('01', '0') => __encoding INCH_R.RS__ // inch_r_rs_
                                when ('01', '1') => __encoding DECH_R.RS__ // dech_r_rs_
                                when ('10', '0') => __encoding INCW_R.RS__ // incw_r_rs_
                                when ('10', '1') => __encoding DECW_R.RS__ // decw_r_rs_
                                when ('11', '0') => __encoding INCD_R.RS__ // incd_r_rs_
                                when ('11', '1') => __encoding DECD_R.RS__ // decd_r_rs_
                        when (_, _, _, '1', _, _, 'x01', _) => __UNPREDICTABLE
                        when (_, _, _, _, _, _, '01x', _) => __UNPREDICTABLE
                        when (_, _, _, _, _, _, '11x', _) => // sve_int_pred_pattern_b
                            __field size 22 +: 2
                            __field sf 20 +: 1
                            __field imm4 16 +: 4
                            __field D 11 +: 1
                            __field U 10 +: 1
                            __field pattern 5 +: 5
                            __field Rdn 0 +: 5
                            case (size, sf, D, U) of
                                when ('00', '0', '0', '0') => __encoding SQINCB_R.RS_SX // sqincb_r_rs_sx
                                when ('00', '0', '0', '1') => __encoding UQINCB_R.RS_UW // uqincb_r_rs_uw
                                when ('00', '0', '1', '0') => __encoding SQDECB_R.RS_SX // sqdecb_r_rs_sx
                                when ('00', '0', '1', '1') => __encoding UQDECB_R.RS_UW // uqdecb_r_rs_uw
                                when ('00', '1', '0', '0') => __encoding SQINCB_R.RS_X // sqincb_r_rs_x
                                when ('00', '1', '0', '1') => __encoding UQINCB_R.RS_X // uqincb_r_rs_x
                                when ('00', '1', '1', '0') => __encoding SQDECB_R.RS_X // sqdecb_r_rs_x
                                when ('00', '1', '1', '1') => __encoding UQDECB_R.RS_X // uqdecb_r_rs_x
                                when ('01', '0', '0', '0') => __encoding SQINCH_R.RS_SX // sqinch_r_rs_sx
                                when ('01', '0', '0', '1') => __encoding UQINCH_R.RS_UW // uqinch_r_rs_uw
                                when ('01', '0', '1', '0') => __encoding SQDECH_R.RS_SX // sqdech_r_rs_sx
                                when ('01', '0', '1', '1') => __encoding UQDECH_R.RS_UW // uqdech_r_rs_uw
                                when ('01', '1', '0', '0') => __encoding SQINCH_R.RS_X // sqinch_r_rs_x
                                when ('01', '1', '0', '1') => __encoding UQINCH_R.RS_X // uqinch_r_rs_x
                                when ('01', '1', '1', '0') => __encoding SQDECH_R.RS_X // sqdech_r_rs_x
                                when ('01', '1', '1', '1') => __encoding UQDECH_R.RS_X // uqdech_r_rs_x
                                when ('10', '0', '0', '0') => __encoding SQINCW_R.RS_SX // sqincw_r_rs_sx
                                when ('10', '0', '0', '1') => __encoding UQINCW_R.RS_UW // uqincw_r_rs_uw
                                when ('10', '0', '1', '0') => __encoding SQDECW_R.RS_SX // sqdecw_r_rs_sx
                                when ('10', '0', '1', '1') => __encoding UQDECW_R.RS_UW // uqdecw_r_rs_uw
                                when ('10', '1', '0', '0') => __encoding SQINCW_R.RS_X // sqincw_r_rs_x
                                when ('10', '1', '0', '1') => __encoding UQINCW_R.RS_X // uqincw_r_rs_x
                                when ('10', '1', '1', '0') => __encoding SQDECW_R.RS_X // sqdecw_r_rs_x
                                when ('10', '1', '1', '1') => __encoding UQDECW_R.RS_X // uqdecw_r_rs_x
                                when ('11', '0', '0', '0') => __encoding SQINCD_R.RS_SX // sqincd_r_rs_sx
                                when ('11', '0', '0', '1') => __encoding UQINCD_R.RS_UW // uqincd_r_rs_uw
                                when ('11', '0', '1', '0') => __encoding SQDECD_R.RS_SX // sqdecd_r_rs_sx
                                when ('11', '0', '1', '1') => __encoding UQDECD_R.RS_UW // uqdecd_r_rs_uw
                                when ('11', '1', '0', '0') => __encoding SQINCD_R.RS_X // sqincd_r_rs_x
                                when ('11', '1', '0', '1') => __encoding UQINCD_R.RS_X // uqincd_r_rs_x
                                when ('11', '1', '1', '0') => __encoding SQDECD_R.RS_X // sqdecd_r_rs_x
                                when ('11', '1', '1', '1') => __encoding UQDECD_R.RS_X // uqdecd_r_rs_x
                when ('000', _, '1x', _, '00xxx', _, _, _) =>
                    // sve_maskimm
                    case (24 +: 8, 22 +: 2, 20 +: 2, 18 +: 2, 0 +: 18) of
                        when (_, _, _, '00', _) => // sve_int_log_imm
                            __field opc 22 +: 2
                            __field imm13 5 +: 13
                            __field Zdn 0 +: 5
                            case (opc) of
                                when ('00') => __encoding ORR_Z.ZI__ // orr_z_zi_
                                when ('01') => __encoding EOR_Z.ZI__ // eor_z_zi_
                                when ('10') => __encoding AND_Z.ZI__ // and_z_zi_
                        when (_, _, _, !'00', _) => __UNPREDICTABLE
                when ('000', _, '1x', _, '01xxx', _, _, _) =>
                    // sve_wideimm_pred
                    case (24 +: 8, 22 +: 2, 20 +: 2, 16 +: 4, 13 +: 3, 0 +: 13) of
                        when (_, _, _, _, '0xx', _) => // sve_int_dup_imm_pred
                            __field size 22 +: 2
                            __field Pg 16 +: 4
                            __field M 14 +: 1
                            __field sh 13 +: 1
                            __field imm8 5 +: 8
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding CPY_Z.P.I__ // cpy_z_p_i_
                        when (_, _, _, _, '10x', _) => __UNPREDICTABLE
                        when (_, _, _, _, '110', _) => // sve_int_dup_fpimm_pred
                            __field size 22 +: 2
                            __field Pg 16 +: 4
                            __field imm8 5 +: 8
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding FCPY_Z.P.I__ // fcpy_z_p_i_
                        when (_, _, _, _, '111', _) => __UNPREDICTABLE
                when ('000', _, '1x', _, '1xxxx', _, '001xxx', _) =>
                    // sve_perm_unpred
                    case (24 +: 8, 22 +: 2, 21 +: 1, 18 +: 3, 16 +: 2, 13 +: 3, 12 +: 1, 10 +: 2, 0 +: 10) of
                        when (_, _, _, '000', '00', _, '1', '10', _) => // sve_int_perm_dup_r
                            __field size 22 +: 2
                            __field Rn 5 +: 5
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding DUP_Z.R__ // dup_z_r_
                        when (_, _, _, '001', '00', _, '1', '10', _) => // sve_int_perm_insrs
                            __field size 22 +: 2
                            __field Rm 5 +: 5
                            __field Zdn 0 +: 5
                            case () of
                                when () => __encoding INSR_Z.R__ // insr_z_r_
                        when (_, _, _, '00x', '00', _, '0', !'00', _) => __UNPREDICTABLE
                        when (_, _, _, '00x', '00', _, '1', 'x1', _) => __UNPREDICTABLE
                        when (_, _, _, '00x', !'00', _, _, '01', _) => __UNPREDICTABLE
                        when (_, _, _, '00x', !'00', _, _, '1x', _) => __UNPREDICTABLE
                        when (_, _, _, '01x', _, _, _, !'00', _) => __UNPREDICTABLE
                        when (_, _, _, '100', _, _, '0', !'00', _) => __UNPREDICTABLE
                        when (_, _, _, '100', _, _, '1', '10', _) => // sve_int_perm_unpk
                            __field size 22 +: 2
                            __field U 17 +: 1
                            __field H 16 +: 1
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (U, H) of
                                when ('0', '0') => __encoding SUNPKLO_Z.Z__ // sunpklo_z_z_
                                when ('0', '1') => __encoding SUNPKHI_Z.Z__ // sunpkhi_z_z_
                                when ('1', '0') => __encoding UUNPKLO_Z.Z__ // uunpklo_z_z_
                                when ('1', '1') => __encoding UUNPKHI_Z.Z__ // uunpkhi_z_z_
                        when (_, _, _, '100', _, _, '1', 'x1', _) => __UNPREDICTABLE
                        when (_, _, _, '101', '00', _, '0', !'00', _) => __UNPREDICTABLE
                        when (_, _, _, '101', '00', _, '1', '10', _) => // sve_int_perm_insrv
                            __field size 22 +: 2
                            __field Vm 5 +: 5
                            __field Zdn 0 +: 5
                            case () of
                                when () => __encoding INSR_Z.V__ // insr_z_v_
                        when (_, _, _, '101', '00', _, '1', 'x1', _) => __UNPREDICTABLE
                        when (_, _, _, '101', !'00', _, _, '01', _) => __UNPREDICTABLE
                        when (_, _, _, '101', !'00', _, _, '1x', _) => __UNPREDICTABLE
                        when (_, _, _, '110', '00', _, '0', !'00', _) => __UNPREDICTABLE
                        when (_, _, _, '110', '00', _, '1', '10', _) => // sve_int_perm_reverse_z
                            __field size 22 +: 2
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding REV_Z.Z__ // rev_z_z_
                        when (_, _, _, '110', '00', _, '1', 'x1', _) => __UNPREDICTABLE
                        when (_, _, _, '110', !'00', _, _, '01', _) => __UNPREDICTABLE
                        when (_, _, _, '110', !'00', _, _, '1x', _) => __UNPREDICTABLE
                        when (_, _, _, '111', _, _, _, _, _) => __UNPREDICTABLE
                        when (_, _, _, _, _, _, '0', '00', _) => // sve_int_perm_dup_i
                            __field imm2 22 +: 2
                            __field tsz 16 +: 5
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding DUP_Z.Zi__ // dup_z_zi_
                        when (_, _, _, _, _, _, '1', '00', _) => // sve_int_perm_tbl
                            __field size 22 +: 2
                            __field Zm 16 +: 5
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding TBL_Z.ZZ_1 // tbl_z_zz_1
                when ('000', _, '1x', _, '1xxxx', _, '010xxx', _) =>
                    // sve_perm_predicates
                    case (24 +: 8, 22 +: 2, 21 +: 1, 16 +: 5, 13 +: 3, 9 +: 4, 5 +: 4, 4 +: 1, 0 +: 4) of
                        when (_, '00', _, '1000x', _, '0000', _, '0', _) => // sve_int_perm_punpk
                            __field H 16 +: 1
                            __field Pn 5 +: 4
                            __field Pd 0 +: 4
                            case (H) of
                                when ('0') => __encoding PUNPKLO_P.P__ // punpklo_p_p_
                                when ('1') => __encoding PUNPKHI_P.P__ // punpkhi_p_p_
                        when (_, '01', _, '1000x', _, '0000', _, '0', _) => __UNPREDICTABLE
                        when (_, '10', _, '1000x', _, '0000', _, '0', _) => __UNPREDICTABLE
                        when (_, '11', _, '1000x', _, '0000', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '0xxxx', _, 'xxx0', _, '0', _) => // sve_int_perm_bin_perm_pp
                            __field size 22 +: 2
                            __field Pm 16 +: 4
                            __field opc 11 +: 2
                            __field H 10 +: 1
                            __field Pn 5 +: 4
                            __field Pd 0 +: 4
                            case (opc, H) of
                                when ('00', '0') => __encoding ZIP1_P.PP__ // zip1_p_pp_
                                when ('00', '1') => __encoding ZIP2_P.PP__ // zip2_p_pp_
                                when ('01', '0') => __encoding UZP1_P.PP__ // uzp1_p_pp_
                                when ('01', '1') => __encoding UZP2_P.PP__ // uzp2_p_pp_
                                when ('10', '0') => __encoding TRN1_P.PP__ // trn1_p_pp_
                                when ('10', '1') => __encoding TRN2_P.PP__ // trn2_p_pp_
                                when ('11', _) => __UNALLOCATED
                        when (_, _, _, '0xxxx', _, 'xxx1', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '10100', _, '0000', _, '0', _) => // sve_int_perm_reverse_p
                            __field size 22 +: 2
                            __field Pn 5 +: 4
                            __field Pd 0 +: 4
                            case () of
                                when () => __encoding REV_P.P__ // rev_p_p_
                        when (_, _, _, '10101', _, '0000', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '10x0x', _, '1000', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '10x0x', _, 'x100', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '10x0x', _, 'xx10', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '10x0x', _, 'xxx1', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '10x1x', _, _, _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '11xxx', _, _, _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, _, _, _, _, '1', _) => __UNPREDICTABLE
                when ('000', _, '1x', _, '1xxxx', _, '011xxx', _) => // sve_int_perm_bin_perm_zz
                    __field size 22 +: 2
                    __field Zm 16 +: 5
                    __field opc 10 +: 3
                    __field Zn 5 +: 5
                    __field Zd 0 +: 5
                    case (opc) of
                        when ('000') => __encoding ZIP1_Z.ZZ__ // zip1_z_zz_
                        when ('001') => __encoding ZIP2_Z.ZZ__ // zip2_z_zz_
                        when ('010') => __encoding UZP1_Z.ZZ__ // uzp1_z_zz_
                        when ('011') => __encoding UZP2_Z.ZZ__ // uzp2_z_zz_
                        when ('100') => __encoding TRN1_Z.ZZ__ // trn1_z_zz_
                        when ('101') => __encoding TRN2_Z.ZZ__ // trn2_z_zz_
                        when ('11x') => __UNALLOCATED
                when ('000', _, '1x', _, '1xxxx', _, '10xxxx', _) =>
                    // sve_perm_pred
                    case (24 +: 8, 23 +: 1, 22 +: 1, 21 +: 1, 20 +: 1, 16 +: 4, 14 +: 2, 13 +: 1, 0 +: 13) of
                        when (_, '0', _, _, '0', '0001', _, '0', _) => __UNPREDICTABLE
                        when (_, '1', _, _, '0', '0001', _, '0', _) => // sve_int_perm_compact
                            __field sz 22 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding COMPACT_Z.P.Z__ // compact_z_p_z_
                        when (_, _, _, _, '0', '0000', _, '0', _) => // sve_int_perm_cpy_v
                            __field size 22 +: 2
                            __field Pg 10 +: 3
                            __field Vn 5 +: 5
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding CPY_Z.P.V__ // cpy_z_p_v_
                        when (_, _, _, _, '0', '000x', _, '1', _) => // sve_int_perm_last_r
                            __field size 22 +: 2
                            __field B 16 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Rd 0 +: 5
                            case (B) of
                                when ('0') => __encoding LASTA_R.P.Z__ // lasta_r_p_z_
                                when ('1') => __encoding LASTB_R.P.Z__ // lastb_r_p_z_
                        when (_, _, _, _, '0', '001x', _, '0', _) => // sve_int_perm_last_v
                            __field size 22 +: 2
                            __field B 16 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Vd 0 +: 5
                            case (B) of
                                when ('0') => __encoding LASTA_V.P.Z__ // lasta_v_p_z_
                                when ('1') => __encoding LASTB_V.P.Z__ // lastb_v_p_z_
                        when (_, _, _, _, '0', '01xx', _, '0', _) => // sve_int_perm_rev
                            __field size 22 +: 2
                            __field opc 16 +: 2
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc) of
                                when ('00') => __encoding REVB_Z.Z__ // revb_z_z_
                                when ('01') => __encoding REVH_Z.Z__ // revh_z_z_
                                when ('10') => __encoding REVW_Z.Z__ // revw_z_z_
                                when ('11') => __encoding RBIT_Z.P.Z__ // rbit_z_p_z_
                        when (_, _, _, _, '0', '01xx', _, '1', _) => __UNPREDICTABLE
                        when (_, _, _, _, '0', '1000', _, '1', _) => // sve_int_perm_cpy_r
                            __field size 22 +: 2
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zd 0 +: 5
                            case () of
                                when () => __encoding CPY_Z.P.R__ // cpy_z_p_r_
                        when (_, _, _, _, '0', '1001', _, '1', _) => __UNPREDICTABLE
                        when (_, _, _, _, '0', '100x', _, '0', _) => // sve_int_perm_clast_zz
                            __field size 22 +: 2
                            __field B 16 +: 1
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case (B) of
                                when ('0') => __encoding CLASTA_Z.P.ZZ__ // clasta_z_p_zz_
                                when ('1') => __encoding CLASTB_Z.P.ZZ__ // clastb_z_p_zz_
                        when (_, _, _, _, '0', '101x', _, '0', _) => // sve_int_perm_clast_vz
                            __field size 22 +: 2
                            __field B 16 +: 1
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Vdn 0 +: 5
                            case (B) of
                                when ('0') => __encoding CLASTA_V.P.Z__ // clasta_v_p_z_
                                when ('1') => __encoding CLASTB_V.P.Z__ // clastb_v_p_z_
                        when (_, _, _, _, '0', '1100', _, '0', _) => // sve_int_perm_splice
                            __field size 22 +: 2
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case () of
                                when () => __encoding SPLICE_Z.P.ZZ_Des // splice_z_p_zz_des
                        when (_, _, _, _, '0', '1100', _, '1', _) => __UNPREDICTABLE
                        when (_, _, _, _, '0', '11!= 00', _, _, _) => __UNPREDICTABLE
                        when (_, _, _, _, '0', 'x01x', _, '1', _) => __UNPREDICTABLE
                        when (_, _, _, _, '1', '000x', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, _, '1', '000x', _, '1', _) => // sve_int_perm_clast_rz
                            __field size 22 +: 2
                            __field B 16 +: 1
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Rdn 0 +: 5
                            case (B) of
                                when ('0') => __encoding CLASTA_R.P.Z__ // clasta_r_p_z_
                                when ('1') => __encoding CLASTB_R.P.Z__ // clastb_r_p_z_
                        when (_, _, _, _, '1', !'000x', _, _, _) => __UNPREDICTABLE
                when ('000', _, '1x', _, '1xxxx', _, '11xxxx', _) => // sve_int_sel_vvv
                    __field size 22 +: 2
                    __field Zm 16 +: 5
                    __field Pg 10 +: 4
                    __field Zn 5 +: 5
                    __field Zd 0 +: 5
                    case () of
                        when () => __encoding SEL_Z.P.ZZ__ // sel_z_p_zz_
                when ('000', _, '10', _, '1xxxx', _, '000xxx', _) =>
                    // sve_perm_extract
                    case (23 +: 9, 22 +: 1, 21 +: 1, 16 +: 5, 13 +: 3, 0 +: 13) of
                        when (_, '0', _, _, _, _) => // sve_int_perm_extract_i
                            __field imm8h 16 +: 5
                            __field imm8l 10 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case () of
                                when () => __encoding EXT_Z.ZI_Des // ext_z_zi_des
                        when (_, '1', _, _, _, _) => __UNPREDICTABLE
                when ('000', _, '11', _, '1xxxx', _, '000xxx', _) => __UNPREDICTABLE
                when ('001', _, '0x', _, '0xxxx', _, _, _) =>
                    // sve_cmpvec
                    case (24 +: 8, 22 +: 2, 21 +: 1, 15 +: 6, 14 +: 1, 0 +: 14) of
                        when (_, _, _, _, '0', _) => // sve_int_cmp_0
                            __field size 22 +: 2
                            __field Zm 16 +: 5
                            __field op 15 +: 1
                            __field o2 13 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field ne 4 +: 1
                            __field Pd 0 +: 4
                            case (op, o2, ne) of
                                when ('0', '0', '0') => __encoding CMPHS_P.P.ZZ__ // cmphs_p_p_zz_
                                when ('0', '0', '1') => __encoding CMPHI_P.P.ZZ__ // cmphi_p_p_zz_
                                when ('0', '1', '0') => __encoding CMPEQ_P.P.ZW__ // cmpeq_p_p_zw_
                                when ('0', '1', '1') => __encoding CMPNE_P.P.ZW__ // cmpne_p_p_zw_
                                when ('1', '0', '0') => __encoding CMPGE_P.P.ZZ__ // cmpge_p_p_zz_
                                when ('1', '0', '1') => __encoding CMPGT_P.P.ZZ__ // cmpgt_p_p_zz_
                                when ('1', '1', '0') => __encoding CMPEQ_P.P.ZZ__ // cmpeq_p_p_zz_
                                when ('1', '1', '1') => __encoding CMPNE_P.P.ZZ__ // cmpne_p_p_zz_
                        when (_, _, _, _, '1', _) => // sve_int_cmp_1
                            __field size 22 +: 2
                            __field Zm 16 +: 5
                            __field U 15 +: 1
                            __field lt 13 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field ne 4 +: 1
                            __field Pd 0 +: 4
                            case (U, lt, ne) of
                                when ('0', '0', '0') => __encoding CMPGE_P.P.ZW__ // cmpge_p_p_zw_
                                when ('0', '0', '1') => __encoding CMPGT_P.P.ZW__ // cmpgt_p_p_zw_
                                when ('0', '1', '0') => __encoding CMPLT_P.P.ZW__ // cmplt_p_p_zw_
                                when ('0', '1', '1') => __encoding CMPLE_P.P.ZW__ // cmple_p_p_zw_
                                when ('1', '0', '0') => __encoding CMPHS_P.P.ZW__ // cmphs_p_p_zw_
                                when ('1', '0', '1') => __encoding CMPHI_P.P.ZW__ // cmphi_p_p_zw_
                                when ('1', '1', '0') => __encoding CMPLO_P.P.ZW__ // cmplo_p_p_zw_
                                when ('1', '1', '1') => __encoding CMPLS_P.P.ZW__ // cmpls_p_p_zw_
                when ('001', _, '0x', _, '1xxxx', _, _, _) => // sve_int_ucmp_vi
                    __field size 22 +: 2
                    __field imm7 14 +: 7
                    __field lt 13 +: 1
                    __field Pg 10 +: 3
                    __field Zn 5 +: 5
                    __field ne 4 +: 1
                    __field Pd 0 +: 4
                    case (lt, ne) of
                        when ('0', '0') => __encoding CMPHS_P.P.ZI__ // cmphs_p_p_zi_
                        when ('0', '1') => __encoding CMPHI_P.P.ZI__ // cmphi_p_p_zi_
                        when ('1', '0') => __encoding CMPLO_P.P.ZI__ // cmplo_p_p_zi_
                        when ('1', '1') => __encoding CMPLS_P.P.ZI__ // cmpls_p_p_zi_
                when ('001', _, '1x', _, '0xxxx', _, 'x0xxxx', _) => // sve_int_scmp_vi
                    __field size 22 +: 2
                    __field imm5 16 +: 5
                    __field op 15 +: 1
                    __field o2 13 +: 1
                    __field Pg 10 +: 3
                    __field Zn 5 +: 5
                    __field ne 4 +: 1
                    __field Pd 0 +: 4
                    case (op, o2, ne) of
                        when ('0', '0', '0') => __encoding CMPGE_P.P.ZI__ // cmpge_p_p_zi_
                        when ('0', '0', '1') => __encoding CMPGT_P.P.ZI__ // cmpgt_p_p_zi_
                        when ('0', '1', '0') => __encoding CMPLT_P.P.ZI__ // cmplt_p_p_zi_
                        when ('0', '1', '1') => __encoding CMPLE_P.P.ZI__ // cmple_p_p_zi_
                        when ('1', '0', '0') => __encoding CMPEQ_P.P.ZI__ // cmpeq_p_p_zi_
                        when ('1', '0', '1') => __encoding CMPNE_P.P.ZI__ // cmpne_p_p_zi_
                        when ('1', '1', _) => __UNALLOCATED
                when ('001', _, '1x', _, '00xxx', _, '01xxxx', _) => // sve_int_pred_log
                    __field op 23 +: 1
                    __field S 22 +: 1
                    __field Pm 16 +: 4
                    __field Pg 10 +: 4
                    __field o2 9 +: 1
                    __field Pn 5 +: 4
                    __field o3 4 +: 1
                    __field Pd 0 +: 4
                    case (op, S, o2, o3) of
                        when ('0', '0', '0', '0') => __encoding AND_P.P.PP_Z // and_p_p_pp_z
                        when ('0', '0', '0', '1') => __encoding BIC_P.P.PP_Z // bic_p_p_pp_z
                        when ('0', '0', '1', '0') => __encoding EOR_P.P.PP_Z // eor_p_p_pp_z
                        when ('0', '0', '1', '1') => __encoding SEL_P.P.PP__ // sel_p_p_pp_
                        when ('0', '1', '0', '0') => __encoding ANDS_P.P.PP_Z // ands_p_p_pp_z
                        when ('0', '1', '0', '1') => __encoding BICS_P.P.PP_Z // bics_p_p_pp_z
                        when ('0', '1', '1', '0') => __encoding EORS_P.P.PP_Z // eors_p_p_pp_z
                        when ('0', '1', '1', '1') => __UNALLOCATED
                        when ('1', '0', '0', '0') => __encoding ORR_P.P.PP_Z // orr_p_p_pp_z
                        when ('1', '0', '0', '1') => __encoding ORN_P.P.PP_Z // orn_p_p_pp_z
                        when ('1', '0', '1', '0') => __encoding NOR_P.P.PP_Z // nor_p_p_pp_z
                        when ('1', '0', '1', '1') => __encoding NAND_P.P.PP_Z // nand_p_p_pp_z
                        when ('1', '1', '0', '0') => __encoding ORRS_P.P.PP_Z // orrs_p_p_pp_z
                        when ('1', '1', '0', '1') => __encoding ORNS_P.P.PP_Z // orns_p_p_pp_z
                        when ('1', '1', '1', '0') => __encoding NORS_P.P.PP_Z // nors_p_p_pp_z
                        when ('1', '1', '1', '1') => __encoding NANDS_P.P.PP_Z // nands_p_p_pp_z
                when ('001', _, '1x', _, '00xxx', _, '11xxxx', _) =>
                    // sve_pred_gen_b
                    case (24 +: 8, 22 +: 2, 20 +: 2, 16 +: 4, 14 +: 2, 10 +: 4, 9 +: 1, 0 +: 9) of
                        when (_, _, _, _, _, _, '0', _) => // sve_int_brkp
                            __field op 23 +: 1
                            __field S 22 +: 1
                            __field Pm 16 +: 4
                            __field Pg 10 +: 4
                            __field Pn 5 +: 4
                            __field B 4 +: 1
                            __field Pd 0 +: 4
                            case (op, S, B) of
                                when ('0', '0', '0') => __encoding BRKPA_P.P.PP__ // brkpa_p_p_pp_
                                when ('0', '0', '1') => __encoding BRKPB_P.P.PP__ // brkpb_p_p_pp_
                                when ('0', '1', '0') => __encoding BRKPAS_P.P.PP__ // brkpas_p_p_pp_
                                when ('0', '1', '1') => __encoding BRKPBS_P.P.PP__ // brkpbs_p_p_pp_
                                when ('1', _, _) => __UNALLOCATED
                        when (_, _, _, _, _, _, '1', _) => __UNPREDICTABLE
                when ('001', _, '1x', _, '01xxx', _, '01xxxx', _) =>
                    // sve_pred_gen_c
                    case (24 +: 8, 23 +: 1, 22 +: 1, 20 +: 2, 16 +: 4, 14 +: 2, 10 +: 4, 9 +: 1, 5 +: 4, 4 +: 1, 0 +: 4) of
                        when (_, '0', _, _, '1000', _, _, '0', _, '0', _) => // sve_int_brkn
                            __field S 22 +: 1
                            __field Pg 10 +: 4
                            __field Pn 5 +: 4
                            __field Pdm 0 +: 4
                            case (S) of
                                when ('0') => __encoding BRKN_P.P.PP__ // brkn_p_p_pp_
                                when ('1') => __encoding BRKNS_P.P.PP__ // brkns_p_p_pp_
                        when (_, '0', _, _, '1000', _, _, '0', _, '1', _) => __UNPREDICTABLE
                        when (_, '0', _, _, 'x000', _, _, '1', _, _, _) => __UNPREDICTABLE
                        when (_, '0', _, _, 'x1xx', _, _, _, _, _, _) => __UNPREDICTABLE
                        when (_, '0', _, _, 'xx1x', _, _, _, _, _, _) => __UNPREDICTABLE
                        when (_, '0', _, _, 'xxx1', _, _, _, _, _, _) => __UNPREDICTABLE
                        when (_, '1', _, _, '0000', _, _, '1', _, _, _) => __UNPREDICTABLE
                        when (_, '1', _, _, !'0000', _, _, _, _, _, _) => __UNPREDICTABLE
                        when (_, _, _, _, '0000', _, _, '0', _, _, _) => // sve_int_break
                            __field B 23 +: 1
                            __field S 22 +: 1
                            __field Pg 10 +: 4
                            __field Pn 5 +: 4
                            __field M 4 +: 1
                            __field Pd 0 +: 4
                            case (B, S, M) of
                                when (_, '1', '1') => __UNALLOCATED
                                when ('0', '0', _) => __encoding BRKA_P.P.P__ // brka_p_p_p_
                                when ('0', '1', '0') => __encoding BRKAS_P.P.P_Z // brkas_p_p_p_z
                                when ('1', '0', _) => __encoding BRKB_P.P.P__ // brkb_p_p_p_
                                when ('1', '1', '0') => __encoding BRKBS_P.P.P_Z // brkbs_p_p_p_z
                when ('001', _, '1x', _, '01xxx', _, '11xxxx', _) =>
                    // sve_pred_gen_d
                    case (24 +: 8, 22 +: 2, 20 +: 2, 16 +: 4, 14 +: 2, 11 +: 3, 9 +: 2, 5 +: 4, 4 +: 1, 0 +: 4) of
                        when (_, _, _, '0000', _, _, 'x0', _, '0', _) => // sve_int_ptest
                            __field op 23 +: 1
                            __field S 22 +: 1
                            __field Pg 10 +: 4
                            __field Pn 5 +: 4
                            __field opc2 0 +: 4
                            case (op, S, opc2) of
                                when ('0', '0', _) => __UNALLOCATED
                                when ('0', '1', '0000') => __encoding PTEST_.P.P__ // ptest_p_p_
                                when ('0', '1', '0001') => __UNALLOCATED
                                when ('0', '1', '001x') => __UNALLOCATED
                                when ('0', '1', '01xx') => __UNALLOCATED
                                when ('0', '1', '1xxx') => __UNALLOCATED
                                when ('1', _, _) => __UNALLOCATED
                        when (_, _, _, '0100', _, _, 'x0', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '0x10', _, _, 'x0', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '0xx1', _, _, 'x0', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '0xxx', _, _, 'x1', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '1000', _, '000', '00', _, '0', _) => // sve_int_pfirst
                            __field op 23 +: 1
                            __field S 22 +: 1
                            __field Pg 5 +: 4
                            __field Pdn 0 +: 4
                            case (op, S) of
                                when ('0', '0') => __UNALLOCATED
                                when ('0', '1') => __encoding PFIRST_P.P.P__ // pfirst_p_p_p_
                                when ('1', _) => __UNALLOCATED
                        when (_, _, _, '1000', _, '000', !'00', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '1000', _, '100', '10', '0000', '0', _) => // sve_int_pfalse
                            __field op 23 +: 1
                            __field S 22 +: 1
                            __field Pd 0 +: 4
                            case (op, S) of
                                when ('0', '0') => __encoding PFALSE_P__ // pfalse_p_
                                when ('0', '1') => __UNALLOCATED
                                when ('1', _) => __UNALLOCATED
                        when (_, _, _, '1000', _, '100', '10', !'0000', '0', _) => __UNPREDICTABLE
                        when (_, _, _, '1000', _, '110', '00', _, '0', _) => // sve_int_rdffr
                            __field op 23 +: 1
                            __field S 22 +: 1
                            __field Pg 5 +: 4
                            __field Pd 0 +: 4
                            case (op, S) of
                                when ('0', '0') => __encoding RDFFR_P.P.F__ // rdffr_p_p_f_
                                when ('0', '1') => __encoding RDFFRS_P.P.F__ // rdffrs_p_p_f_
                                when ('1', _) => __UNALLOCATED
                        when (_, _, _, '1001', _, '000', '0x', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '1001', _, '000', '10', _, '0', _) => // sve_int_pnext
                            __field size 22 +: 2
                            __field Pg 5 +: 4
                            __field Pdn 0 +: 4
                            case () of
                                when () => __encoding PNEXT_P.P.P__ // pnext_p_p_p_
                        when (_, _, _, '1001', _, '000', '11', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '1001', _, '100', '10', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '1001', _, '110', '00', '0000', '0', _) => // sve_int_rdffr_2
                            __field op 23 +: 1
                            __field S 22 +: 1
                            __field Pd 0 +: 4
                            case (op, S) of
                                when ('0', '0') => __encoding RDFFR_P.F__ // rdffr_p_f_
                                when ('0', '1') => __UNALLOCATED
                                when ('1', _) => __UNALLOCATED
                        when (_, _, _, '1001', _, '110', '00', !'0000', '0', _) => __UNPREDICTABLE
                        when (_, _, _, '100x', _, '010', _, _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '100x', _, '100', '0x', _, '0', _) => // sve_int_ptrue
                            __field size 22 +: 2
                            __field S 16 +: 1
                            __field pattern 5 +: 5
                            __field Pd 0 +: 4
                            case (S) of
                                when ('0') => __encoding PTRUE_P.S__ // ptrue_p_s_
                                when ('1') => __encoding PTRUES_P.S__ // ptrues_p_s_
                        when (_, _, _, '100x', _, '100', '11', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '100x', _, '110', !'00', _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '100x', _, 'xx1', _, _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '110x', _, _, _, _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, '1x1x', _, _, _, _, '0', _) => __UNPREDICTABLE
                        when (_, _, _, _, _, _, _, _, '1', _) => __UNPREDICTABLE
                when ('001', _, '1x', _, '1xxxx', _, '00xxxx', _) =>
                    // sve_cmpgpr
                    case (24 +: 8, 22 +: 2, 21 +: 1, 16 +: 5, 14 +: 2, 13 +: 1, 10 +: 3, 4 +: 6, 0 +: 4) of
                        when (_, _, _, _, _, '0', _, _, _) => // sve_int_while_rr
                            __field size 22 +: 2
                            __field Rm 16 +: 5
                            __field sf 12 +: 1
                            __field U 11 +: 1
                            __field lt 10 +: 1
                            __field Rn 5 +: 5
                            __field eq 4 +: 1
                            __field Pd 0 +: 4
                            case (U, lt, eq) of
                                when (_, '0', _) => __UNALLOCATED
                                when ('0', '1', '0') => __encoding WHILELT_P.P.RR__ // whilelt_p_p_rr_
                                when ('0', '1', '1') => __encoding WHILELE_P.P.RR__ // whilele_p_p_rr_
                                when ('1', '1', '0') => __encoding WHILELO_P.P.RR__ // whilelo_p_p_rr_
                                when ('1', '1', '1') => __encoding WHILELS_P.P.RR__ // whilels_p_p_rr_
                        when (_, _, _, _, _, '1', '000', _, '0000') => // sve_int_cterm
                            __field op 23 +: 1
                            __field sz 22 +: 1
                            __field Rm 16 +: 5
                            __field Rn 5 +: 5
                            __field ne 4 +: 1
                            case (op, ne) of
                                when ('0', _) => __UNALLOCATED
                                when ('1', '0') => __encoding CTERMEQ_RR__ // ctermeq_rr_
                                when ('1', '1') => __encoding CTERMNE_RR__ // ctermne_rr_
                        when (_, _, _, _, _, '1', '000', _, !'0000') => __UNPREDICTABLE
                        when (_, _, _, _, _, '1', !'000', _, _) => __UNPREDICTABLE
                when ('001', _, '1x', _, '1xxxx', _, '01xxxx', _) => __UNPREDICTABLE
                when ('001', _, '1x', _, '1xxxx', _, '11xxxx', _) =>
                    // sve_wideimm_unpred
                    case (24 +: 8, 22 +: 2, 21 +: 1, 19 +: 2, 17 +: 2, 16 +: 1, 14 +: 2, 0 +: 14) of
                        when (_, _, _, '00', _, _, _, _) => // sve_int_arith_imm0
                            __field size 22 +: 2
                            __field opc 16 +: 3
                            __field sh 13 +: 1
                            __field imm8 5 +: 8
                            __field Zdn 0 +: 5
                            case (opc) of
                                when ('000') => __encoding ADD_Z.ZI__ // add_z_zi_
                                when ('001') => __encoding SUB_Z.ZI__ // sub_z_zi_
                                when ('010') => __UNALLOCATED
                                when ('011') => __encoding SUBR_Z.ZI__ // subr_z_zi_
                                when ('100') => __encoding SQADD_Z.ZI__ // sqadd_z_zi_
                                when ('101') => __encoding UQADD_Z.ZI__ // uqadd_z_zi_
                                when ('110') => __encoding SQSUB_Z.ZI__ // sqsub_z_zi_
                                when ('111') => __encoding UQSUB_Z.ZI__ // uqsub_z_zi_
                        when (_, _, _, '01', _, _, _, _) => // sve_int_arith_imm1
                            __field size 22 +: 2
                            __field opc 16 +: 3
                            __field o2 13 +: 1
                            __field imm8 5 +: 8
                            __field Zdn 0 +: 5
                            case (opc, o2) of
                                when ('0xx', '1') => __UNALLOCATED
                                when ('000', '0') => __encoding SMAX_Z.ZI__ // smax_z_zi_
                                when ('001', '0') => __encoding UMAX_Z.ZI__ // umax_z_zi_
                                when ('010', '0') => __encoding SMIN_Z.ZI__ // smin_z_zi_
                                when ('011', '0') => __encoding UMIN_Z.ZI__ // umin_z_zi_
                                when ('1xx', _) => __UNALLOCATED
                        when (_, _, _, '10', _, _, _, _) => // sve_int_arith_imm2
                            __field size 22 +: 2
                            __field opc 16 +: 3
                            __field o2 13 +: 1
                            __field imm8 5 +: 8
                            __field Zdn 0 +: 5
                            case (opc, o2) of
                                when ('000', '0') => __encoding MUL_Z.ZI__ // mul_z_zi_
                                when ('000', '1') => __UNALLOCATED
                                when ('001', _) => __UNALLOCATED
                                when ('01x', _) => __UNALLOCATED
                                when ('1xx', _) => __UNALLOCATED
                        when (_, _, _, '11', _, '0', _, _) => // sve_int_dup_imm
                            __field size 22 +: 2
                            __field opc 17 +: 2
                            __field sh 13 +: 1
                            __field imm8 5 +: 8
                            __field Zd 0 +: 5
                            case (opc) of
                                when ('00') => __encoding DUP_Z.I__ // dup_z_i_
                                when ('01') => __UNALLOCATED
                                when ('1x') => __UNALLOCATED
                        when (_, _, _, '11', _, '1', _, _) => // sve_int_dup_fpimm
                            __field size 22 +: 2
                            __field opc 17 +: 2
                            __field o2 13 +: 1
                            __field imm8 5 +: 8
                            __field Zd 0 +: 5
                            case (opc, o2) of
                                when ('00', '0') => __encoding FDUP_Z.I__ // fdup_z_i_
                                when ('00', '1') => __UNALLOCATED
                                when ('01', _) => __UNALLOCATED
                                when ('1x', _) => __UNALLOCATED
                when ('001', _, '1x', _, '100xx', _, '10xxxx', _) => // sve_int_pcount_pred
                    __field size 22 +: 2
                    __field opc 16 +: 3
                    __field Pg 10 +: 4
                    __field o2 9 +: 1
                    __field Pn 5 +: 4
                    __field Rd 0 +: 5
                    case (opc, o2) of
                        when ('000', '0') => __encoding CNTP_R.P.P__ // cntp_r_p_p_
                        when ('000', '1') => __UNALLOCATED
                        when ('001', _) => __UNALLOCATED
                        when ('01x', _) => __UNALLOCATED
                        when ('1xx', _) => __UNALLOCATED
                when ('001', _, '1x', _, '101xx', _, '1000xx', _) =>
                    // sve_pred_count_b
                    case (24 +: 8, 22 +: 2, 19 +: 3, 18 +: 1, 16 +: 2, 12 +: 4, 11 +: 1, 0 +: 11) of
                        when (_, _, _, '0', _, _, '0', _) => // sve_int_count_v_sat
                            __field size 22 +: 2
                            __field D 17 +: 1
                            __field U 16 +: 1
                            __field opc 9 +: 2
                            __field Pg 5 +: 4
                            __field Zdn 0 +: 5
                            case (D, U, opc) of
                                when (_, _, '01') => __UNALLOCATED
                                when (_, _, '1x') => __UNALLOCATED
                                when ('0', '0', '00') => __encoding SQINCP_Z.P.Z__ // sqincp_z_p_z_
                                when ('0', '1', '00') => __encoding UQINCP_Z.P.Z__ // uqincp_z_p_z_
                                when ('1', '0', '00') => __encoding SQDECP_Z.P.Z__ // sqdecp_z_p_z_
                                when ('1', '1', '00') => __encoding UQDECP_Z.P.Z__ // uqdecp_z_p_z_
                        when (_, _, _, '0', _, _, '1', _) => // sve_int_count_r_sat
                            __field size 22 +: 2
                            __field D 17 +: 1
                            __field U 16 +: 1
                            __field sf 10 +: 1
                            __field op 9 +: 1
                            __field Pg 5 +: 4
                            __field Rdn 0 +: 5
                            case (D, U, sf, op) of
                                when (_, _, _, '1') => __UNALLOCATED
                                when ('0', '0', '0', '0') => __encoding SQINCP_R.P.R_SX // sqincp_r_p_r_sx
                                when ('0', '0', '1', '0') => __encoding SQINCP_R.P.R_X // sqincp_r_p_r_x
                                when ('0', '1', '0', '0') => __encoding UQINCP_R.P.R_UW // uqincp_r_p_r_uw
                                when ('0', '1', '1', '0') => __encoding UQINCP_R.P.R_X // uqincp_r_p_r_x
                                when ('1', '0', '0', '0') => __encoding SQDECP_R.P.R_SX // sqdecp_r_p_r_sx
                                when ('1', '0', '1', '0') => __encoding SQDECP_R.P.R_X // sqdecp_r_p_r_x
                                when ('1', '1', '0', '0') => __encoding UQDECP_R.P.R_UW // uqdecp_r_p_r_uw
                                when ('1', '1', '1', '0') => __encoding UQDECP_R.P.R_X // uqdecp_r_p_r_x
                        when (_, _, _, '1', _, _, '0', _) => // sve_int_count_v
                            __field size 22 +: 2
                            __field op 17 +: 1
                            __field D 16 +: 1
                            __field opc2 9 +: 2
                            __field Pg 5 +: 4
                            __field Zdn 0 +: 5
                            case (op, D, opc2) of
                                when ('0', _, '01') => __UNALLOCATED
                                when ('0', _, '1x') => __UNALLOCATED
                                when ('0', '0', '00') => __encoding INCP_Z.P.Z__ // incp_z_p_z_
                                when ('0', '1', '00') => __encoding DECP_Z.P.Z__ // decp_z_p_z_
                                when ('1', _, _) => __UNALLOCATED
                        when (_, _, _, '1', _, _, '1', _) => // sve_int_count_r
                            __field size 22 +: 2
                            __field op 17 +: 1
                            __field D 16 +: 1
                            __field opc2 9 +: 2
                            __field Pg 5 +: 4
                            __field Rdn 0 +: 5
                            case (op, D, opc2) of
                                when ('0', _, '01') => __UNALLOCATED
                                when ('0', _, '1x') => __UNALLOCATED
                                when ('0', '0', '00') => __encoding INCP_R.P.R__ // incp_r_p_r_
                                when ('0', '1', '00') => __encoding DECP_R.P.R__ // decp_r_p_r_
                                when ('1', _, _) => __UNALLOCATED
                when ('001', _, '1x', _, '101xx', _, '1001xx', _) =>
                    // sve_pred_wrffr
                    case (24 +: 8, 22 +: 2, 19 +: 3, 18 +: 1, 16 +: 2, 12 +: 4, 9 +: 3, 5 +: 4, 0 +: 5) of
                        when (_, _, _, '0', '00', _, '000', _, '00000') => // sve_int_wrffr
                            __field opc 22 +: 2
                            __field Pn 5 +: 4
                            case (opc) of
                                when ('00') => __encoding WRFFR_F.P__ // wrffr_f_p_
                                when ('01') => __UNALLOCATED
                                when ('1x') => __UNALLOCATED
                        when (_, _, _, '1', '00', _, '000', '0000', '00000') => // sve_int_setffr
                            __field opc 22 +: 2
                            case (opc) of
                                when ('00') => __encoding SETFFR_F__ // setffr_f_
                                when ('01') => __UNALLOCATED
                                when ('1x') => __UNALLOCATED
                        when (_, _, _, '1', '00', _, '000', '1xxx', '00000') => __UNPREDICTABLE
                        when (_, _, _, '1', '00', _, '000', 'x1xx', '00000') => __UNPREDICTABLE
                        when (_, _, _, '1', '00', _, '000', 'xx1x', '00000') => __UNPREDICTABLE
                        when (_, _, _, '1', '00', _, '000', 'xxx1', '00000') => __UNPREDICTABLE
                        when (_, _, _, _, '00', _, '000', _, !'00000') => __UNPREDICTABLE
                        when (_, _, _, _, '00', _, !'000', _, _) => __UNPREDICTABLE
                        when (_, _, _, _, !'00', _, _, _, _) => __UNPREDICTABLE
                when ('001', _, '1x', _, '101xx', _, '101xxx', _) => __UNPREDICTABLE
                when ('001', _, '1x', _, '11xxx', _, '10xxxx', _) => __UNPREDICTABLE
                when ('010', _, '0x', _, '0xxxx', _, '0xxxxx', _) =>
                    // sve_intx_muladd_unpred
                    case (24 +: 8, 22 +: 2, 21 +: 1, 16 +: 5, 15 +: 1, 11 +: 4, 0 +: 11) of
                        when (_, _, _, _, _, '0000', _) => // sve_intx_dot
                            __field size 22 +: 2
                            __field Zm 16 +: 5
                            __field U 10 +: 1
                            __field Zn 5 +: 5
                            __field Zda 0 +: 5
                            case (U) of
                                when ('0') => __encoding SDOT_Z.ZZZ__ // sdot_z_zzz_
                                when ('1') => __encoding UDOT_Z.ZZZ__ // udot_z_zzz_
                        when (_, _, _, _, _, !'0000', _) => __UNPREDICTABLE
                when ('010', _, '0x', _, '0xxxx', _, '1xxxxx', _) => __UNPREDICTABLE
                when ('010', _, '0x', _, '1xxxx', _, _, _) =>
                    // sve_intx_by_indexed_elem
                    case (24 +: 8, 22 +: 2, 21 +: 1, 16 +: 5, 11 +: 5, 0 +: 11) of
                        when (_, _, _, _, '00000', _) => // sve_intx_dot_by_indexed_elem
                            __field size 22 +: 2
                            __field opc 16 +: 5
                            __field U 10 +: 1
                            __field Zn 5 +: 5
                            __field Zda 0 +: 5
                            case (size, U) of
                                when ('0x', _) => __UNALLOCATED
                                when ('10', '0') => __encoding SDOT_Z.ZZZi_S // sdot_z_zzzi_s
                                when ('10', '1') => __encoding UDOT_Z.ZZZi_S // udot_z_zzzi_s
                                when ('11', '0') => __encoding SDOT_Z.ZZZi_D // sdot_z_zzzi_d
                                when ('11', '1') => __encoding UDOT_Z.ZZZi_D // udot_z_zzzi_d
                        when (_, _, _, _, !'00000', _) => __UNPREDICTABLE
                when ('010', _, '1x', _, _, _, _, _) => __UNPREDICTABLE
                when ('011', _, '0x', _, '0xxxx', _, '0xxxxx', _) => // sve_fp_fcmla
                    __field size 22 +: 2
                    __field Zm 16 +: 5
                    __field rot 13 +: 2
                    __field Pg 10 +: 3
                    __field Zn 5 +: 5
                    __field Zda 0 +: 5
                    case () of
                        when () => __encoding FCMLA_Z.P.ZZZ__ // fcmla_z_p_zzz_
                when ('011', _, '0x', _, '00000', _, '100xxx', _) => // sve_fp_fcadd
                    __field size 22 +: 2
                    __field rot 16 +: 1
                    __field Pg 10 +: 3
                    __field Zm 5 +: 5
                    __field Zdn 0 +: 5
                    case () of
                        when () => __encoding FCADD_Z.P.ZZ__ // fcadd_z_p_zz_
                when ('011', _, '0x', _, '00000', _, '101xxx', _) => __UNPREDICTABLE
                when ('011', _, '0x', _, '00000', _, '11xxxx', _) => __UNPREDICTABLE
                when ('011', _, '0x', _, '00001', _, '1xxxxx', _) => __UNPREDICTABLE
                when ('011', _, '0x', _, '0001x', _, '1xxxxx', _) => __UNPREDICTABLE
                when ('011', _, '0x', _, '001xx', _, '1xxxxx', _) => __UNPREDICTABLE
                when ('011', _, '0x', _, '01xxx', _, '1xxxxx', _) => __UNPREDICTABLE
                when ('011', _, '0x', _, '1xxxx', _, '00x01x', _) => __UNPREDICTABLE
                when ('011', _, '0x', _, '1xxxx', _, '00000x', _) => // sve_fp_fma_by_indexed_elem
                    __field size 22 +: 2
                    __field opc 16 +: 5
                    __field op 10 +: 1
                    __field Zn 5 +: 5
                    __field Zda 0 +: 5
                    case (size, op) of
                        when ('0x', '0') => __encoding FMLA_Z.ZZZi_H // fmla_z_zzzi_h
                        when ('0x', '1') => __encoding FMLS_Z.ZZZi_H // fmls_z_zzzi_h
                        when ('10', '0') => __encoding FMLA_Z.ZZZi_S // fmla_z_zzzi_s
                        when ('10', '1') => __encoding FMLS_Z.ZZZi_S // fmls_z_zzzi_s
                        when ('11', '0') => __encoding FMLA_Z.ZZZi_D // fmla_z_zzzi_d
                        when ('11', '1') => __encoding FMLS_Z.ZZZi_D // fmls_z_zzzi_d
                when ('011', _, '0x', _, '1xxxx', _, '0001xx', _) => // sve_fp_fcmla_by_indexed_elem
                    __field size 22 +: 2
                    __field opc 16 +: 5
                    __field rot 10 +: 2
                    __field Zn 5 +: 5
                    __field Zda 0 +: 5
                    case (size) of
                        when ('0x') => __UNALLOCATED
                        when ('10') => __encoding FCMLA_Z.ZZZi_H // fcmla_z_zzzi_h
                        when ('11') => __encoding FCMLA_Z.ZZZi_S // fcmla_z_zzzi_s
                when ('011', _, '0x', _, '1xxxx', _, '001000', _) => // sve_fp_fmul_by_indexed_elem
                    __field size 22 +: 2
                    __field opc 16 +: 5
                    __field Zn 5 +: 5
                    __field Zd 0 +: 5
                    case (size) of
                        when ('0x') => __encoding FMUL_Z.ZZi_H // fmul_z_zzi_h
                        when ('10') => __encoding FMUL_Z.ZZi_S // fmul_z_zzi_s
                        when ('11') => __encoding FMUL_Z.ZZi_D // fmul_z_zzi_d
                when ('011', _, '0x', _, '1xxxx', _, '001001', _) => __UNPREDICTABLE
                when ('011', _, '0x', _, '1xxxx', _, '0011xx', _) => __UNPREDICTABLE
                when ('011', _, '0x', _, '1xxxx', _, '01xxxx', _) => __UNPREDICTABLE
                when ('011', _, '0x', _, '1xxxx', _, '1xxxxx', _) => __UNPREDICTABLE
                when ('011', _, '1x', _, '0xxxx', _, 'x1xxxx', _) => // sve_fp_3op_p_pd
                    __field size 22 +: 2
                    __field Zm 16 +: 5
                    __field op 15 +: 1
                    __field o2 13 +: 1
                    __field Pg 10 +: 3
                    __field Zn 5 +: 5
                    __field o3 4 +: 1
                    __field Pd 0 +: 4
                    case (op, o2, o3) of
                        when ('0', '0', '0') => __encoding FCMGE_P.P.ZZ__ // fcmge_p_p_zz_
                        when ('0', '0', '1') => __encoding FCMGT_P.P.ZZ__ // fcmgt_p_p_zz_
                        when ('0', '1', '0') => __encoding FCMEQ_P.P.ZZ__ // fcmeq_p_p_zz_
                        when ('0', '1', '1') => __encoding FCMNE_P.P.ZZ__ // fcmne_p_p_zz_
                        when ('1', '0', '0') => __encoding FCMUO_P.P.ZZ__ // fcmuo_p_p_zz_
                        when ('1', '0', '1') => __encoding FACGE_P.P.ZZ__ // facge_p_p_zz_
                        when ('1', '1', '0') => __UNALLOCATED
                        when ('1', '1', '1') => __encoding FACGT_P.P.ZZ__ // facgt_p_p_zz_
                when ('011', _, '1x', _, '0xxxx', _, '000xxx', _) => // sve_fp_3op_u_zd
                    __field size 22 +: 2
                    __field Zm 16 +: 5
                    __field opc 10 +: 3
                    __field Zn 5 +: 5
                    __field Zd 0 +: 5
                    case (opc) of
                        when ('000') => __encoding FADD_Z.ZZ__ // fadd_z_zz_
                        when ('001') => __encoding FSUB_Z.ZZ__ // fsub_z_zz_
                        when ('010') => __encoding FMUL_Z.ZZ__ // fmul_z_zz_
                        when ('011') => __encoding FTSMUL_Z.ZZ__ // ftsmul_z_zz_
                        when ('10x') => __UNALLOCATED
                        when ('110') => __encoding FRECPS_Z.ZZ__ // frecps_z_zz_
                        when ('111') => __encoding FRSQRTS_Z.ZZ__ // frsqrts_z_zz_
                when ('011', _, '1x', _, '0xxxx', _, '100xxx', _) =>
                    // sve_fp_pred
                    case (24 +: 8, 22 +: 2, 21 +: 1, 19 +: 2, 16 +: 3, 13 +: 3, 10 +: 3, 6 +: 4, 0 +: 6) of
                        when (_, _, _, '0x', _, _, _, _, _) => // sve_fp_2op_p_zds
                            __field size 22 +: 2
                            __field opc 16 +: 4
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case (opc) of
                                when ('0000') => __encoding FADD_Z.P.ZZ__ // fadd_z_p_zz_
                                when ('0001') => __encoding FSUB_Z.P.ZZ__ // fsub_z_p_zz_
                                when ('0010') => __encoding FMUL_Z.P.ZZ__ // fmul_z_p_zz_
                                when ('0011') => __encoding FSUBR_Z.P.ZZ__ // fsubr_z_p_zz_
                                when ('0100') => __encoding FMAXNM_Z.P.ZZ__ // fmaxnm_z_p_zz_
                                when ('0101') => __encoding FMINNM_Z.P.ZZ__ // fminnm_z_p_zz_
                                when ('0110') => __encoding FMAX_Z.P.ZZ__ // fmax_z_p_zz_
                                when ('0111') => __encoding FMIN_Z.P.ZZ__ // fmin_z_p_zz_
                                when ('1000') => __encoding FABD_Z.P.ZZ__ // fabd_z_p_zz_
                                when ('1001') => __encoding FSCALE_Z.P.ZZ__ // fscale_z_p_zz_
                                when ('1010') => __encoding FMULX_Z.P.ZZ__ // fmulx_z_p_zz_
                                when ('1011') => __UNALLOCATED
                                when ('1100') => __encoding FDIVR_Z.P.ZZ__ // fdivr_z_p_zz_
                                when ('1101') => __encoding FDIV_Z.P.ZZ__ // fdiv_z_p_zz_
                                when ('111x') => __UNALLOCATED
                        when (_, _, _, '10', _, _, '000', _, _) => // sve_fp_ftmad
                            __field size 22 +: 2
                            __field imm3 16 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case () of
                                when () => __encoding FTMAD_Z.ZZI__ // ftmad_z_zzi_
                        when (_, _, _, '10', _, _, !'000', _, _) => __UNPREDICTABLE
                        when (_, _, _, '11', _, _, _, '0000', _) => // sve_fp_2op_i_p_zds
                            __field size 22 +: 2
                            __field opc 16 +: 3
                            __field Pg 10 +: 3
                            __field i1 5 +: 1
                            __field Zdn 0 +: 5
                            case (opc) of
                                when ('000') => __encoding FADD_Z.P.ZS__ // fadd_z_p_zs_
                                when ('001') => __encoding FSUB_Z.P.ZS__ // fsub_z_p_zs_
                                when ('010') => __encoding FMUL_Z.P.ZS__ // fmul_z_p_zs_
                                when ('011') => __encoding FSUBR_Z.P.ZS__ // fsubr_z_p_zs_
                                when ('100') => __encoding FMAXNM_Z.P.ZS__ // fmaxnm_z_p_zs_
                                when ('101') => __encoding FMINNM_Z.P.ZS__ // fminnm_z_p_zs_
                                when ('110') => __encoding FMAX_Z.P.ZS__ // fmax_z_p_zs_
                                when ('111') => __encoding FMIN_Z.P.ZS__ // fmin_z_p_zs_
                        when (_, _, _, '11', _, _, _, !'0000', _) => __UNPREDICTABLE
                when ('011', _, '1x', _, '0xxxx', _, '101xxx', _) =>
                    // sve_fp_unary
                    case (24 +: 8, 22 +: 2, 21 +: 1, 18 +: 3, 16 +: 2, 13 +: 3, 0 +: 13) of
                        when (_, _, _, '00x', _, _, _) => // sve_fp_2op_p_zd_a
                            __field size 22 +: 2
                            __field opc 16 +: 3
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc) of
                                when ('000') => __encoding FRINTN_Z.P.Z__ // frintn_z_p_z_
                                when ('001') => __encoding FRINTP_Z.P.Z__ // frintp_z_p_z_
                                when ('010') => __encoding FRINTM_Z.P.Z__ // frintm_z_p_z_
                                when ('011') => __encoding FRINTZ_Z.P.Z__ // frintz_z_p_z_
                                when ('100') => __encoding FRINTA_Z.P.Z__ // frinta_z_p_z_
                                when ('101') => __UNALLOCATED
                                when ('110') => __encoding FRINTX_Z.P.Z__ // frintx_z_p_z_
                                when ('111') => __encoding FRINTI_Z.P.Z__ // frinti_z_p_z_
                        when (_, _, _, '010', _, _, _) => // sve_fp_2op_p_zd_b_0
                            __field opc 22 +: 2
                            __field opc2 16 +: 2
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc, opc2) of
                                when ('0x', _) => __UNALLOCATED
                                when ('10', '00') => __encoding FCVT_Z.P.Z_S2H // fcvt_z_p_z_s2h
                                when ('10', '01') => __encoding FCVT_Z.P.Z_H2S // fcvt_z_p_z_h2s
                                when ('10', '1x') => __UNALLOCATED
                                when ('11', '00') => __encoding FCVT_Z.P.Z_D2H // fcvt_z_p_z_d2h
                                when ('11', '01') => __encoding FCVT_Z.P.Z_H2D // fcvt_z_p_z_h2d
                                when ('11', '10') => __encoding FCVT_Z.P.Z_D2S // fcvt_z_p_z_d2s
                                when ('11', '11') => __encoding FCVT_Z.P.Z_S2D // fcvt_z_p_z_s2d
                        when (_, _, _, '011', _, _, _) => // sve_fp_2op_p_zd_b_1
                            __field size 22 +: 2
                            __field opc 16 +: 2
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc) of
                                when ('00') => __encoding FRECPX_Z.P.Z__ // frecpx_z_p_z_
                                when ('01') => __encoding FSQRT_Z.P.Z__ // fsqrt_z_p_z_
                                when ('1x') => __UNALLOCATED
                        when (_, _, _, '10x', _, _, _) => // sve_fp_2op_p_zd_c
                            __field opc 22 +: 2
                            __field opc2 17 +: 2
                            __field U 16 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc, opc2, U) of
                                when ('00', _, _) => __UNALLOCATED
                                when ('01', '00', _) => __UNALLOCATED
                                when ('01', '01', '0') => __encoding SCVTF_Z.P.Z_H2FP16 // scvtf_z_p_z_h2fp16
                                when ('01', '01', '1') => __encoding UCVTF_Z.P.Z_H2FP16 // ucvtf_z_p_z_h2fp16
                                when ('01', '10', '0') => __encoding SCVTF_Z.P.Z_W2FP16 // scvtf_z_p_z_w2fp16
                                when ('01', '10', '1') => __encoding UCVTF_Z.P.Z_W2FP16 // ucvtf_z_p_z_w2fp16
                                when ('01', '11', '0') => __encoding SCVTF_Z.P.Z_X2FP16 // scvtf_z_p_z_x2fp16
                                when ('01', '11', '1') => __encoding UCVTF_Z.P.Z_X2FP16 // ucvtf_z_p_z_x2fp16
                                when ('10', '0x', _) => __UNALLOCATED
                                when ('10', '10', '0') => __encoding SCVTF_Z.P.Z_W2S // scvtf_z_p_z_w2s
                                when ('10', '10', '1') => __encoding UCVTF_Z.P.Z_W2S // ucvtf_z_p_z_w2s
                                when ('10', '11', _) => __UNALLOCATED
                                when ('11', '00', '0') => __encoding SCVTF_Z.P.Z_W2D // scvtf_z_p_z_w2d
                                when ('11', '00', '1') => __encoding UCVTF_Z.P.Z_W2D // ucvtf_z_p_z_w2d
                                when ('11', '01', _) => __UNALLOCATED
                                when ('11', '10', '0') => __encoding SCVTF_Z.P.Z_X2S // scvtf_z_p_z_x2s
                                when ('11', '10', '1') => __encoding UCVTF_Z.P.Z_X2S // ucvtf_z_p_z_x2s
                                when ('11', '11', '0') => __encoding SCVTF_Z.P.Z_X2D // scvtf_z_p_z_x2d
                                when ('11', '11', '1') => __encoding UCVTF_Z.P.Z_X2D // ucvtf_z_p_z_x2d
                        when (_, _, _, '11x', _, _, _) => // sve_fp_2op_p_zd_d
                            __field opc 22 +: 2
                            __field opc2 17 +: 2
                            __field U 16 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc, opc2, U) of
                                when ('00', _, _) => __UNALLOCATED
                                when ('01', '00', _) => __UNALLOCATED
                                when ('01', '01', '0') => __encoding FCVTZS_Z.P.Z_FP162H // fcvtzs_z_p_z_fp162h
                                when ('01', '01', '1') => __encoding FCVTZU_Z.P.Z_FP162H // fcvtzu_z_p_z_fp162h
                                when ('01', '10', '0') => __encoding FCVTZS_Z.P.Z_FP162W // fcvtzs_z_p_z_fp162w
                                when ('01', '10', '1') => __encoding FCVTZU_Z.P.Z_FP162W // fcvtzu_z_p_z_fp162w
                                when ('01', '11', '0') => __encoding FCVTZS_Z.P.Z_FP162X // fcvtzs_z_p_z_fp162x
                                when ('01', '11', '1') => __encoding FCVTZU_Z.P.Z_FP162X // fcvtzu_z_p_z_fp162x
                                when ('10', '0x', _) => __UNALLOCATED
                                when ('10', '10', '0') => __encoding FCVTZS_Z.P.Z_S2W // fcvtzs_z_p_z_s2w
                                when ('10', '10', '1') => __encoding FCVTZU_Z.P.Z_S2W // fcvtzu_z_p_z_s2w
                                when ('10', '11', _) => __UNALLOCATED
                                when ('11', '00', '0') => __encoding FCVTZS_Z.P.Z_D2W // fcvtzs_z_p_z_d2w
                                when ('11', '00', '1') => __encoding FCVTZU_Z.P.Z_D2W // fcvtzu_z_p_z_d2w
                                when ('11', '01', _) => __UNALLOCATED
                                when ('11', '10', '0') => __encoding FCVTZS_Z.P.Z_S2X // fcvtzs_z_p_z_s2x
                                when ('11', '10', '1') => __encoding FCVTZU_Z.P.Z_S2X // fcvtzu_z_p_z_s2x
                                when ('11', '11', '0') => __encoding FCVTZS_Z.P.Z_D2X // fcvtzs_z_p_z_d2x
                                when ('11', '11', '1') => __encoding FCVTZU_Z.P.Z_D2X // fcvtzu_z_p_z_d2x
                when ('011', _, '1x', _, '000xx', _, '001xxx', _) => // sve_fp_fast_red
                    __field size 22 +: 2
                    __field opc 16 +: 3
                    __field Pg 10 +: 3
                    __field Zn 5 +: 5
                    __field Vd 0 +: 5
                    case (opc) of
                        when ('000') => __encoding FADDV_V.P.Z__ // faddv_v_p_z_
                        when ('001') => __UNALLOCATED
                        when ('01x') => __UNALLOCATED
                        when ('100') => __encoding FMAXNMV_V.P.Z__ // fmaxnmv_v_p_z_
                        when ('101') => __encoding FMINNMV_V.P.Z__ // fminnmv_v_p_z_
                        when ('110') => __encoding FMAXV_V.P.Z__ // fmaxv_v_p_z_
                        when ('111') => __encoding FMINV_V.P.Z__ // fminv_v_p_z_
                when ('011', _, '1x', _, '001xx', _, '0010xx', _) => __UNPREDICTABLE
                when ('011', _, '1x', _, '001xx', _, '0011xx', _) =>
                    // sve_fp_unary_unpred
                    case (24 +: 8, 22 +: 2, 19 +: 3, 16 +: 3, 12 +: 4, 10 +: 2, 0 +: 10) of
                        when (_, _, _, _, _, '00', _) => // sve_fp_2op_u_zd
                            __field size 22 +: 2
                            __field opc 16 +: 3
                            __field Zn 5 +: 5
                            __field Zd 0 +: 5
                            case (opc) of
                                when ('0xx') => __UNALLOCATED
                                when ('10x') => __UNALLOCATED
                                when ('110') => __encoding FRECPE_Z.Z__ // frecpe_z_z_
                                when ('111') => __encoding FRSQRTE_Z.Z__ // frsqrte_z_z_
                        when (_, _, _, _, _, !'00', _) => __UNPREDICTABLE
                when ('011', _, '1x', _, '010xx', _, '001xxx', _) =>
                    // sve_fp_cmpzero
                    case (24 +: 8, 22 +: 2, 19 +: 3, 18 +: 1, 16 +: 2, 13 +: 3, 0 +: 13) of
                        when (_, _, _, '0', _, _, _) => // sve_fp_2op_p_pd
                            __field size 22 +: 2
                            __field eq 17 +: 1
                            __field lt 16 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field ne 4 +: 1
                            __field Pd 0 +: 4
                            case (eq, lt, ne) of
                                when ('0', '0', '0') => __encoding FCMGE_P.P.Z0__ // fcmge_p_p_z0_
                                when ('0', '0', '1') => __encoding FCMGT_P.P.Z0__ // fcmgt_p_p_z0_
                                when ('0', '1', '0') => __encoding FCMLT_P.P.Z0__ // fcmlt_p_p_z0_
                                when ('0', '1', '1') => __encoding FCMLE_P.P.Z0__ // fcmle_p_p_z0_
                                when ('1', _, '1') => __UNALLOCATED
                                when ('1', '0', '0') => __encoding FCMEQ_P.P.Z0__ // fcmeq_p_p_z0_
                                when ('1', '1', '0') => __encoding FCMNE_P.P.Z0__ // fcmne_p_p_z0_
                        when (_, _, _, '1', _, _, _) => __UNPREDICTABLE
                when ('011', _, '1x', _, '011xx', _, '001xxx', _) => // sve_fp_2op_p_vd
                    __field size 22 +: 2
                    __field opc 16 +: 3
                    __field Pg 10 +: 3
                    __field Zm 5 +: 5
                    __field Vdn 0 +: 5
                    case (opc) of
                        when ('000') => __encoding FADDA_V.P.Z__ // fadda_v_p_z_
                        when ('001') => __UNALLOCATED
                        when ('01x') => __UNALLOCATED
                        when ('1xx') => __UNALLOCATED
                when ('011', _, '1x', _, '1xxxx', _, _, _) =>
                    // sve_fp_fma
                    case (24 +: 8, 22 +: 2, 21 +: 1, 16 +: 5, 15 +: 1, 0 +: 15) of
                        when (_, _, _, _, '0', _) => // sve_fp_3op_p_zds_a
                            __field size 22 +: 2
                            __field Zm 16 +: 5
                            __field opc 13 +: 2
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zda 0 +: 5
                            case (opc) of
                                when ('00') => __encoding FMLA_Z.P.ZZZ__ // fmla_z_p_zzz_
                                when ('01') => __encoding FMLS_Z.P.ZZZ__ // fmls_z_p_zzz_
                                when ('10') => __encoding FNMLA_Z.P.ZZZ__ // fnmla_z_p_zzz_
                                when ('11') => __encoding FNMLS_Z.P.ZZZ__ // fnmls_z_p_zzz_
                        when (_, _, _, _, '1', _) => // sve_fp_3op_p_zds_b
                            __field size 22 +: 2
                            __field Za 16 +: 5
                            __field opc 13 +: 2
                            __field Pg 10 +: 3
                            __field Zm 5 +: 5
                            __field Zdn 0 +: 5
                            case (opc) of
                                when ('00') => __encoding FMAD_Z.P.ZZZ__ // fmad_z_p_zzz_
                                when ('01') => __encoding FMSB_Z.P.ZZZ__ // fmsb_z_p_zzz_
                                when ('10') => __encoding FNMAD_Z.P.ZZZ__ // fnmad_z_p_zzz_
                                when ('11') => __encoding FNMSB_Z.P.ZZZ__ // fnmsb_z_p_zzz_
                when ('100', _, _, _, _, _, _, _) =>
                    // sve_mem32
                    case (25 +: 7, 21 +: 4, 16 +: 5, 13 +: 3, 5 +: 8, 4 +: 1, 0 +: 4) of
                        when (_, '00x1', _, '0xx', _, '0', _) => // sve_mem_32b_prfm_sv
                            __field xs 22 +: 1
                            __field Zm 16 +: 5
                            __field msz 13 +: 2
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field prfop 0 +: 4
                            case (msz) of
                                when ('00') => __encoding PRFB_I.P.BZ_S.x32.scaled // prfb_i_p_bz_s_x32_scaled
                                when ('01') => __encoding PRFH_I.P.BZ_S.x32.scaled // prfh_i_p_bz_s_x32_scaled
                                when ('10') => __encoding PRFW_I.P.BZ_S.x32.scaled // prfw_i_p_bz_s_x32_scaled
                                when ('11') => __encoding PRFD_I.P.BZ_S.x32.scaled // prfd_i_p_bz_s_x32_scaled
                        when (_, '00x1', _, '0xx', _, '1', _) => __UNPREDICTABLE
                        when (_, '01x1', _, '0xx', _, _, _) => // sve_mem_32b_gld_sv_a
                            __field xs 22 +: 1
                            __field Zm 16 +: 5
                            __field U 14 +: 1
                            __field ff 13 +: 1
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (U, ff) of
                                when ('0', '0') => __encoding LD1SH_Z.P.BZ_S.x32.scaled // ld1sh_z_p_bz_s_x32_scaled
                                when ('0', '1') => __encoding LDFF1SH_Z.P.BZ_S.x32.scaled // ldff1sh_z_p_bz_s_x32_scaled
                                when ('1', '0') => __encoding LD1H_Z.P.BZ_S.x32.scaled // ld1h_z_p_bz_s_x32_scaled
                                when ('1', '1') => __encoding LDFF1H_Z.P.BZ_S.x32.scaled // ldff1h_z_p_bz_s_x32_scaled
                        when (_, '10x1', _, '0xx', _, _, _) => // sve_mem_32b_gld_sv_b
                            __field xs 22 +: 1
                            __field Zm 16 +: 5
                            __field U 14 +: 1
                            __field ff 13 +: 1
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (U, ff) of
                                when ('0', _) => __UNALLOCATED
                                when ('1', '0') => __encoding LD1W_Z.P.BZ_S.x32.scaled // ld1w_z_p_bz_s_x32_scaled
                                when ('1', '1') => __encoding LDFF1W_Z.P.BZ_S.x32.scaled // ldff1w_z_p_bz_s_x32_scaled
                        when (_, '1101', _, '000', _, '1', _) => __UNPREDICTABLE
                        when (_, '1101', _, '0x1', _, _, _) => __UNPREDICTABLE
                        when (_, '110x', _, '000', _, '0', _) => // sve_mem_32b_pfill
                            __field imm9h 16 +: 6
                            __field imm9l 10 +: 3
                            __field Rn 5 +: 5
                            __field Pt 0 +: 4
                            case () of
                                when () => __encoding LDR_P.BI__ // ldr_p_bi_
                        when (_, '110x', _, '010', _, _, _) => // sve_mem_32b_fill
                            __field imm9h 16 +: 6
                            __field imm9l 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case () of
                                when () => __encoding LDR_Z.BI__ // ldr_z_bi_
                        when (_, '1111', _, '0xx', _, '1', _) => __UNPREDICTABLE
                        when (_, '111x', _, '0xx', _, '0', _) => // sve_mem_prfm_si
                            __field imm6 16 +: 6
                            __field msz 13 +: 2
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field prfop 0 +: 4
                            case (msz) of
                                when ('00') => __encoding PRFB_I.P.BI_S // prfb_i_p_bi_s
                                when ('01') => __encoding PRFH_I.P.BI_S // prfh_i_p_bi_s
                                when ('10') => __encoding PRFW_I.P.BI_S // prfw_i_p_bi_s
                                when ('11') => __encoding PRFD_I.P.BI_S // prfd_i_p_bi_s
                        when (_, 'xx00', _, '10x', _, _, _) => __UNPREDICTABLE
                        when (_, 'xx00', _, '110', _, '0', _) => // sve_mem_prfm_ss
                            __field msz 23 +: 2
                            __field Rm 16 +: 5
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field prfop 0 +: 4
                            case (msz) of
                                when ('00') => __encoding PRFB_I.P.BR_S // prfb_i_p_br_s
                                when ('01') => __encoding PRFH_I.P.BR_S // prfh_i_p_br_s
                                when ('10') => __encoding PRFW_I.P.BR_S // prfw_i_p_br_s
                                when ('11') => __encoding PRFD_I.P.BR_S // prfd_i_p_br_s
                        when (_, 'xx00', _, '111', _, '0', _) => // sve_mem_32b_prfm_vi
                            __field msz 23 +: 2
                            __field imm5 16 +: 5
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field prfop 0 +: 4
                            case (msz) of
                                when ('00') => __encoding PRFB_I.P.AI_S // prfb_i_p_ai_s
                                when ('01') => __encoding PRFH_I.P.AI_S // prfh_i_p_ai_s
                                when ('10') => __encoding PRFW_I.P.AI_S // prfw_i_p_ai_s
                                when ('11') => __encoding PRFD_I.P.AI_S // prfd_i_p_ai_s
                        when (_, 'xx00', _, '11x', _, '1', _) => __UNPREDICTABLE
                        when (_, 'xx01', _, '1xx', _, _, _) => // sve_mem_32b_gld_vi
                            __field msz 23 +: 2
                            __field imm5 16 +: 5
                            __field U 14 +: 1
                            __field ff 13 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, U, ff) of
                                when ('00', '0', '0') => __encoding LD1SB_Z.P.AI_S // ld1sb_z_p_ai_s
                                when ('00', '0', '1') => __encoding LDFF1SB_Z.P.AI_S // ldff1sb_z_p_ai_s
                                when ('00', '1', '0') => __encoding LD1B_Z.P.AI_S // ld1b_z_p_ai_s
                                when ('00', '1', '1') => __encoding LDFF1B_Z.P.AI_S // ldff1b_z_p_ai_s
                                when ('01', '0', '0') => __encoding LD1SH_Z.P.AI_S // ld1sh_z_p_ai_s
                                when ('01', '0', '1') => __encoding LDFF1SH_Z.P.AI_S // ldff1sh_z_p_ai_s
                                when ('01', '1', '0') => __encoding LD1H_Z.P.AI_S // ld1h_z_p_ai_s
                                when ('01', '1', '1') => __encoding LDFF1H_Z.P.AI_S // ldff1h_z_p_ai_s
                                when ('10', '0', _) => __UNALLOCATED
                                when ('10', '1', '0') => __encoding LD1W_Z.P.AI_S // ld1w_z_p_ai_s
                                when ('10', '1', '1') => __encoding LDFF1W_Z.P.AI_S // ldff1w_z_p_ai_s
                                when ('11', _, _) => __UNALLOCATED
                        when (_, 'xx1x', _, '1xx', _, _, _) => // sve_mem_ld_dup
                            __field dtypeh 23 +: 2
                            __field imm6 16 +: 6
                            __field dtypel 13 +: 2
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (dtype1h, dtype1l) of
                                when ('00', '00') => __encoding LD1RB_Z.P.BI_U8 // ld1rb_z_p_bi_u8
                                when ('00', '01') => __encoding LD1RB_Z.P.BI_U16 // ld1rb_z_p_bi_u16
                                when ('00', '10') => __encoding LD1RB_Z.P.BI_U32 // ld1rb_z_p_bi_u32
                                when ('00', '11') => __encoding LD1RB_Z.P.BI_U64 // ld1rb_z_p_bi_u64
                                when ('01', '00') => __encoding LD1RSW_Z.P.BI_S64 // ld1rsw_z_p_bi_s64
                                when ('01', '01') => __encoding LD1RH_Z.P.BI_U16 // ld1rh_z_p_bi_u16
                                when ('01', '10') => __encoding LD1RH_Z.P.BI_U32 // ld1rh_z_p_bi_u32
                                when ('01', '11') => __encoding LD1RH_Z.P.BI_U64 // ld1rh_z_p_bi_u64
                                when ('10', '00') => __encoding LD1RSH_Z.P.BI_S64 // ld1rsh_z_p_bi_s64
                                when ('10', '01') => __encoding LD1RSH_Z.P.BI_S32 // ld1rsh_z_p_bi_s32
                                when ('10', '10') => __encoding LD1RW_Z.P.BI_U32 // ld1rw_z_p_bi_u32
                                when ('10', '11') => __encoding LD1RW_Z.P.BI_U64 // ld1rw_z_p_bi_u64
                                when ('11', '00') => __encoding LD1RSB_Z.P.BI_S64 // ld1rsb_z_p_bi_s64
                                when ('11', '01') => __encoding LD1RSB_Z.P.BI_S32 // ld1rsb_z_p_bi_s32
                                when ('11', '10') => __encoding LD1RSB_Z.P.BI_S16 // ld1rsb_z_p_bi_s16
                                when ('11', '11') => __encoding LD1RD_Z.P.BI_U64 // ld1rd_z_p_bi_u64
                        when (_, 'xxx0', _, '0xx', _, _, _) => // sve_mem_32b_gld_vs
                            __field msz 23 +: 2
                            __field xs 22 +: 1
                            __field Zm 16 +: 5
                            __field U 14 +: 1
                            __field ff 13 +: 1
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, xs, U, ff, Zt) of
                                when ('00', _, '0', '0', _) => __encoding LD1SB_Z.P.BZ_S.x32.unscaled // ld1sb_z_p_bz_s_x32_unscaled
                                when ('00', _, '0', '1', _) => __encoding LDFF1SB_Z.P.BZ_S.x32.unscaled // ldff1sb_z_p_bz_s_x32_unscaled
                                when ('00', _, '1', '0', _) => __encoding LD1B_Z.P.BZ_S.x32.unscaled // ld1b_z_p_bz_s_x32_unscaled
                                when ('00', _, '1', '1', _) => __encoding LDFF1B_Z.P.BZ_S.x32.unscaled // ldff1b_z_p_bz_s_x32_unscaled
                                when ('01', _, '0', '0', _) => __encoding LD1SH_Z.P.BZ_S.x32.unscaled // ld1sh_z_p_bz_s_x32_unscaled
                                when ('01', _, '0', '1', _) => __encoding LDFF1SH_Z.P.BZ_S.x32.unscaled // ldff1sh_z_p_bz_s_x32_unscaled
                                when ('01', _, '1', '0', _) => __encoding LD1H_Z.P.BZ_S.x32.unscaled // ld1h_z_p_bz_s_x32_unscaled
                                when ('01', _, '1', '1', _) => __encoding LDFF1H_Z.P.BZ_S.x32.unscaled // ldff1h_z_p_bz_s_x32_unscaled
                                when ('10', _, '0', _, _) => __UNALLOCATED
                                when ('10', _, '1', '0', _) => __encoding LD1W_Z.P.BZ_S.x32.unscaled // ld1w_z_p_bz_s_x32_unscaled
                                when ('10', _, '1', '1', _) => __encoding LDFF1W_Z.P.BZ_S.x32.unscaled // ldff1w_z_p_bz_s_x32_unscaled
                                when ('11', '0', _, '1', _) => __UNALLOCATED
                                when ('11', '0', '0', '0', '1xxxx') => __UNALLOCATED
                                when ('11', '1', _, _, '1xxxx') => __UNALLOCATED
                when ('101', _, _, _, _, _, _, _) =>
                    // sve_memcld
                    case (25 +: 7, 21 +: 4, 20 +: 1, 16 +: 4, 13 +: 3, 0 +: 13) of
                        when (_, _, '0', _, '001', _) => // sve_mem_ldqr_si
                            __field msz 23 +: 2
                            __field num 21 +: 2
                            __field imm4 16 +: 4
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, num) of
                                when (_, '01') => __UNALLOCATED
                                when (_, '1x') => __UNALLOCATED
                                when ('00', '00') => __encoding LD1RQB_Z.P.BI_U8 // ld1rqb_z_p_bi_u8
                                when ('01', '00') => __encoding LD1RQH_Z.P.BI_U16 // ld1rqh_z_p_bi_u16
                                when ('10', '00') => __encoding LD1RQW_Z.P.BI_U32 // ld1rqw_z_p_bi_u32
                                when ('11', '00') => __encoding LD1RQD_Z.P.BI_U64 // ld1rqd_z_p_bi_u64
                        when (_, _, '0', _, '101', _) => // sve_mem_cld_si
                            __field dtype 21 +: 4
                            __field imm4 16 +: 4
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (dtype1) of
                                when ('0000') => __encoding LD1B_Z.P.BI_U8 // ld1b_z_p_bi_u8
                                when ('0001') => __encoding LD1B_Z.P.BI_U16 // ld1b_z_p_bi_u16
                                when ('0010') => __encoding LD1B_Z.P.BI_U32 // ld1b_z_p_bi_u32
                                when ('0011') => __encoding LD1B_Z.P.BI_U64 // ld1b_z_p_bi_u64
                                when ('0100') => __encoding LD1SW_Z.P.BI_S64 // ld1sw_z_p_bi_s64
                                when ('0101') => __encoding LD1H_Z.P.BI_U16 // ld1h_z_p_bi_u16
                                when ('0110') => __encoding LD1H_Z.P.BI_U32 // ld1h_z_p_bi_u32
                                when ('0111') => __encoding LD1H_Z.P.BI_U64 // ld1h_z_p_bi_u64
                                when ('1000') => __encoding LD1SH_Z.P.BI_S64 // ld1sh_z_p_bi_s64
                                when ('1001') => __encoding LD1SH_Z.P.BI_S32 // ld1sh_z_p_bi_s32
                                when ('1010') => __encoding LD1W_Z.P.BI_U32 // ld1w_z_p_bi_u32
                                when ('1011') => __encoding LD1W_Z.P.BI_U64 // ld1w_z_p_bi_u64
                                when ('1100') => __encoding LD1SB_Z.P.BI_S64 // ld1sb_z_p_bi_s64
                                when ('1101') => __encoding LD1SB_Z.P.BI_S32 // ld1sb_z_p_bi_s32
                                when ('1110') => __encoding LD1SB_Z.P.BI_S16 // ld1sb_z_p_bi_s16
                                when ('1111') => __encoding LD1D_Z.P.BI_U64 // ld1d_z_p_bi_u64
                        when (_, _, '0', _, '111', _) => // sve_mem_eld_si
                            __field msz 23 +: 2
                            __field num 21 +: 2
                            __field imm4 16 +: 4
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, num) of
                                when ('00', '01') => __encoding LD2B_Z.P.BI_Contiguous // ld2b_z_p_bi_contiguous
                                when ('00', '10') => __encoding LD3B_Z.P.BI_Contiguous // ld3b_z_p_bi_contiguous
                                when ('00', '11') => __encoding LD4B_Z.P.BI_Contiguous // ld4b_z_p_bi_contiguous
                                when ('01', '01') => __encoding LD2H_Z.P.BI_Contiguous // ld2h_z_p_bi_contiguous
                                when ('01', '10') => __encoding LD3H_Z.P.BI_Contiguous // ld3h_z_p_bi_contiguous
                                when ('01', '11') => __encoding LD4H_Z.P.BI_Contiguous // ld4h_z_p_bi_contiguous
                                when ('10', '01') => __encoding LD2W_Z.P.BI_Contiguous // ld2w_z_p_bi_contiguous
                                when ('10', '10') => __encoding LD3W_Z.P.BI_Contiguous // ld3w_z_p_bi_contiguous
                                when ('10', '11') => __encoding LD4W_Z.P.BI_Contiguous // ld4w_z_p_bi_contiguous
                                when ('11', '01') => __encoding LD2D_Z.P.BI_Contiguous // ld2d_z_p_bi_contiguous
                                when ('11', '10') => __encoding LD3D_Z.P.BI_Contiguous // ld3d_z_p_bi_contiguous
                                when ('11', '11') => __encoding LD4D_Z.P.BI_Contiguous // ld4d_z_p_bi_contiguous
                        when (_, _, '1', _, '001', _) => __UNPREDICTABLE
                        when (_, _, '1', _, '101', _) => // sve_mem_cldnf_si
                            __field dtype 21 +: 4
                            __field imm4 16 +: 4
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (dtype1) of
                                when ('0000') => __encoding LDNF1B_Z.P.BI_U8 // ldnf1b_z_p_bi_u8
                                when ('0001') => __encoding LDNF1B_Z.P.BI_U16 // ldnf1b_z_p_bi_u16
                                when ('0010') => __encoding LDNF1B_Z.P.BI_U32 // ldnf1b_z_p_bi_u32
                                when ('0011') => __encoding LDNF1B_Z.P.BI_U64 // ldnf1b_z_p_bi_u64
                                when ('0100') => __encoding LDNF1SW_Z.P.BI_S64 // ldnf1sw_z_p_bi_s64
                                when ('0101') => __encoding LDNF1H_Z.P.BI_U16 // ldnf1h_z_p_bi_u16
                                when ('0110') => __encoding LDNF1H_Z.P.BI_U32 // ldnf1h_z_p_bi_u32
                                when ('0111') => __encoding LDNF1H_Z.P.BI_U64 // ldnf1h_z_p_bi_u64
                                when ('1000') => __encoding LDNF1SH_Z.P.BI_S64 // ldnf1sh_z_p_bi_s64
                                when ('1001') => __encoding LDNF1SH_Z.P.BI_S32 // ldnf1sh_z_p_bi_s32
                                when ('1010') => __encoding LDNF1W_Z.P.BI_U32 // ldnf1w_z_p_bi_u32
                                when ('1011') => __encoding LDNF1W_Z.P.BI_U64 // ldnf1w_z_p_bi_u64
                                when ('1100') => __encoding LDNF1SB_Z.P.BI_S64 // ldnf1sb_z_p_bi_s64
                                when ('1101') => __encoding LDNF1SB_Z.P.BI_S32 // ldnf1sb_z_p_bi_s32
                                when ('1110') => __encoding LDNF1SB_Z.P.BI_S16 // ldnf1sb_z_p_bi_s16
                                when ('1111') => __encoding LDNF1D_Z.P.BI_U64 // ldnf1d_z_p_bi_u64
                        when (_, _, '1', _, '111', _) => __UNPREDICTABLE
                        when (_, _, _, _, '000', _) => // sve_mem_ldqr_ss
                            __field msz 23 +: 2
                            __field num 21 +: 2
                            __field Rm 16 +: 5
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, num) of
                                when (_, '01') => __UNALLOCATED
                                when (_, '1x') => __UNALLOCATED
                                when ('00', '00') => __encoding LD1RQB_Z.P.BR_Contiguous // ld1rqb_z_p_br_contiguous
                                when ('01', '00') => __encoding LD1RQH_Z.P.BR_Contiguous // ld1rqh_z_p_br_contiguous
                                when ('10', '00') => __encoding LD1RQW_Z.P.BR_Contiguous // ld1rqw_z_p_br_contiguous
                                when ('11', '00') => __encoding LD1RQD_Z.P.BR_Contiguous // ld1rqd_z_p_br_contiguous
                        when (_, _, _, _, '010', _) => // sve_mem_cld_ss
                            __field dtype 21 +: 4
                            __field Rm 16 +: 5
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (dtype1) of
                                when ('0000') => __encoding LD1B_Z.P.BR_U8 // ld1b_z_p_br_u8
                                when ('0001') => __encoding LD1B_Z.P.BR_U16 // ld1b_z_p_br_u16
                                when ('0010') => __encoding LD1B_Z.P.BR_U32 // ld1b_z_p_br_u32
                                when ('0011') => __encoding LD1B_Z.P.BR_U64 // ld1b_z_p_br_u64
                                when ('0100') => __encoding LD1SW_Z.P.BR_S64 // ld1sw_z_p_br_s64
                                when ('0101') => __encoding LD1H_Z.P.BR_U16 // ld1h_z_p_br_u16
                                when ('0110') => __encoding LD1H_Z.P.BR_U32 // ld1h_z_p_br_u32
                                when ('0111') => __encoding LD1H_Z.P.BR_U64 // ld1h_z_p_br_u64
                                when ('1000') => __encoding LD1SH_Z.P.BR_S64 // ld1sh_z_p_br_s64
                                when ('1001') => __encoding LD1SH_Z.P.BR_S32 // ld1sh_z_p_br_s32
                                when ('1010') => __encoding LD1W_Z.P.BR_U32 // ld1w_z_p_br_u32
                                when ('1011') => __encoding LD1W_Z.P.BR_U64 // ld1w_z_p_br_u64
                                when ('1100') => __encoding LD1SB_Z.P.BR_S64 // ld1sb_z_p_br_s64
                                when ('1101') => __encoding LD1SB_Z.P.BR_S32 // ld1sb_z_p_br_s32
                                when ('1110') => __encoding LD1SB_Z.P.BR_S16 // ld1sb_z_p_br_s16
                                when ('1111') => __encoding LD1D_Z.P.BR_U64 // ld1d_z_p_br_u64
                        when (_, _, _, _, '011', _) => // sve_mem_cldff_ss
                            __field dtype 21 +: 4
                            __field Rm 16 +: 5
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (dtype1) of
                                when ('0000') => __encoding LDFF1B_Z.P.BR_U8 // ldff1b_z_p_br_u8
                                when ('0001') => __encoding LDFF1B_Z.P.BR_U16 // ldff1b_z_p_br_u16
                                when ('0010') => __encoding LDFF1B_Z.P.BR_U32 // ldff1b_z_p_br_u32
                                when ('0011') => __encoding LDFF1B_Z.P.BR_U64 // ldff1b_z_p_br_u64
                                when ('0100') => __encoding LDFF1SW_Z.P.BR_S64 // ldff1sw_z_p_br_s64
                                when ('0101') => __encoding LDFF1H_Z.P.BR_U16 // ldff1h_z_p_br_u16
                                when ('0110') => __encoding LDFF1H_Z.P.BR_U32 // ldff1h_z_p_br_u32
                                when ('0111') => __encoding LDFF1H_Z.P.BR_U64 // ldff1h_z_p_br_u64
                                when ('1000') => __encoding LDFF1SH_Z.P.BR_S64 // ldff1sh_z_p_br_s64
                                when ('1001') => __encoding LDFF1SH_Z.P.BR_S32 // ldff1sh_z_p_br_s32
                                when ('1010') => __encoding LDFF1W_Z.P.BR_U32 // ldff1w_z_p_br_u32
                                when ('1011') => __encoding LDFF1W_Z.P.BR_U64 // ldff1w_z_p_br_u64
                                when ('1100') => __encoding LDFF1SB_Z.P.BR_S64 // ldff1sb_z_p_br_s64
                                when ('1101') => __encoding LDFF1SB_Z.P.BR_S32 // ldff1sb_z_p_br_s32
                                when ('1110') => __encoding LDFF1SB_Z.P.BR_S16 // ldff1sb_z_p_br_s16
                                when ('1111') => __encoding LDFF1D_Z.P.BR_U64 // ldff1d_z_p_br_u64
                        when (_, _, _, _, '100', _) => __UNPREDICTABLE
                        when (_, _, _, _, '110', _) => // sve_mem_eld_ss
                            __field msz 23 +: 2
                            __field num 21 +: 2
                            __field Rm 16 +: 5
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, num) of
                                when ('00', '01') => __encoding LD2B_Z.P.BR_Contiguous // ld2b_z_p_br_contiguous
                                when ('00', '10') => __encoding LD3B_Z.P.BR_Contiguous // ld3b_z_p_br_contiguous
                                when ('00', '11') => __encoding LD4B_Z.P.BR_Contiguous // ld4b_z_p_br_contiguous
                                when ('01', '01') => __encoding LD2H_Z.P.BR_Contiguous // ld2h_z_p_br_contiguous
                                when ('01', '10') => __encoding LD3H_Z.P.BR_Contiguous // ld3h_z_p_br_contiguous
                                when ('01', '11') => __encoding LD4H_Z.P.BR_Contiguous // ld4h_z_p_br_contiguous
                                when ('10', '01') => __encoding LD2W_Z.P.BR_Contiguous // ld2w_z_p_br_contiguous
                                when ('10', '10') => __encoding LD3W_Z.P.BR_Contiguous // ld3w_z_p_br_contiguous
                                when ('10', '11') => __encoding LD4W_Z.P.BR_Contiguous // ld4w_z_p_br_contiguous
                                when ('11', '01') => __encoding LD2D_Z.P.BR_Contiguous // ld2d_z_p_br_contiguous
                                when ('11', '10') => __encoding LD3D_Z.P.BR_Contiguous // ld3d_z_p_br_contiguous
                                when ('11', '11') => __encoding LD4D_Z.P.BR_Contiguous // ld4d_z_p_br_contiguous
                when ('110', _, _, _, _, _, _, _) =>
                    // sve_mem64
                    case (25 +: 7, 23 +: 2, 21 +: 2, 16 +: 5, 13 +: 3, 5 +: 8, 4 +: 1, 0 +: 4) of
                        when (_, _, '00', _, '10x', _, _, _) => __UNPREDICTABLE
                        when (_, _, '00', _, '110', _, _, _) => __UNPREDICTABLE
                        when (_, _, '00', _, '111', _, '0', _) => // sve_mem_64b_prfm_vi
                            __field msz 23 +: 2
                            __field imm5 16 +: 5
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field prfop 0 +: 4
                            case (msz) of
                                when ('00') => __encoding PRFB_I.P.AI_D // prfb_i_p_ai_d
                                when ('01') => __encoding PRFH_I.P.AI_D // prfh_i_p_ai_d
                                when ('10') => __encoding PRFW_I.P.AI_D // prfw_i_p_ai_d
                                when ('11') => __encoding PRFD_I.P.AI_D // prfd_i_p_ai_d
                        when (_, _, '00', _, '111', _, '1', _) => __UNPREDICTABLE
                        when (_, _, '01', _, '1xx', _, _, _) => // sve_mem_64b_gld_vi
                            __field msz 23 +: 2
                            __field imm5 16 +: 5
                            __field U 14 +: 1
                            __field ff 13 +: 1
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, U, ff) of
                                when ('00', '0', '0') => __encoding LD1SB_Z.P.AI_D // ld1sb_z_p_ai_d
                                when ('00', '0', '1') => __encoding LDFF1SB_Z.P.AI_D // ldff1sb_z_p_ai_d
                                when ('00', '1', '0') => __encoding LD1B_Z.P.AI_D // ld1b_z_p_ai_d
                                when ('00', '1', '1') => __encoding LDFF1B_Z.P.AI_D // ldff1b_z_p_ai_d
                                when ('01', '0', '0') => __encoding LD1SH_Z.P.AI_D // ld1sh_z_p_ai_d
                                when ('01', '0', '1') => __encoding LDFF1SH_Z.P.AI_D // ldff1sh_z_p_ai_d
                                when ('01', '1', '0') => __encoding LD1H_Z.P.AI_D // ld1h_z_p_ai_d
                                when ('01', '1', '1') => __encoding LDFF1H_Z.P.AI_D // ldff1h_z_p_ai_d
                                when ('10', '0', '0') => __encoding LD1SW_Z.P.AI_D // ld1sw_z_p_ai_d
                                when ('10', '0', '1') => __encoding LDFF1SW_Z.P.AI_D // ldff1sw_z_p_ai_d
                                when ('10', '1', '0') => __encoding LD1W_Z.P.AI_D // ld1w_z_p_ai_d
                                when ('10', '1', '1') => __encoding LDFF1W_Z.P.AI_D // ldff1w_z_p_ai_d
                                when ('11', '0', _) => __UNALLOCATED
                                when ('11', '1', '0') => __encoding LD1D_Z.P.AI_D // ld1d_z_p_ai_d
                                when ('11', '1', '1') => __encoding LDFF1D_Z.P.AI_D // ldff1d_z_p_ai_d
                        when (_, _, '10', _, '1xx', _, _, _) => // sve_mem_64b_gld_vs2
                            __field msz 23 +: 2
                            __field Zm 16 +: 5
                            __field U 14 +: 1
                            __field ff 13 +: 1
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, U, ff) of
                                when ('00', '0', '0') => __encoding LD1SB_Z.P.BZ_D.64.unscaled // ld1sb_z_p_bz_d_64_unscaled
                                when ('00', '0', '1') => __encoding LDFF1SB_Z.P.BZ_D.64.unscaled // ldff1sb_z_p_bz_d_64_unscaled
                                when ('00', '1', '0') => __encoding LD1B_Z.P.BZ_D.64.unscaled // ld1b_z_p_bz_d_64_unscaled
                                when ('00', '1', '1') => __encoding LDFF1B_Z.P.BZ_D.64.unscaled // ldff1b_z_p_bz_d_64_unscaled
                                when ('01', '0', '0') => __encoding LD1SH_Z.P.BZ_D.64.unscaled // ld1sh_z_p_bz_d_64_unscaled
                                when ('01', '0', '1') => __encoding LDFF1SH_Z.P.BZ_D.64.unscaled // ldff1sh_z_p_bz_d_64_unscaled
                                when ('01', '1', '0') => __encoding LD1H_Z.P.BZ_D.64.unscaled // ld1h_z_p_bz_d_64_unscaled
                                when ('01', '1', '1') => __encoding LDFF1H_Z.P.BZ_D.64.unscaled // ldff1h_z_p_bz_d_64_unscaled
                                when ('10', '0', '0') => __encoding LD1SW_Z.P.BZ_D.64.unscaled // ld1sw_z_p_bz_d_64_unscaled
                                when ('10', '0', '1') => __encoding LDFF1SW_Z.P.BZ_D.64.unscaled // ldff1sw_z_p_bz_d_64_unscaled
                                when ('10', '1', '0') => __encoding LD1W_Z.P.BZ_D.64.unscaled // ld1w_z_p_bz_d_64_unscaled
                                when ('10', '1', '1') => __encoding LDFF1W_Z.P.BZ_D.64.unscaled // ldff1w_z_p_bz_d_64_unscaled
                                when ('11', '0', _) => __UNALLOCATED
                                when ('11', '1', '0') => __encoding LD1D_Z.P.BZ_D.64.unscaled // ld1d_z_p_bz_d_64_unscaled
                                when ('11', '1', '1') => __encoding LDFF1D_Z.P.BZ_D.64.unscaled // ldff1d_z_p_bz_d_64_unscaled
                        when (_, _, '11', _, '1xx', _, _, _) => // sve_mem_64b_gld_sv2
                            __field msz 23 +: 2
                            __field Zm 16 +: 5
                            __field U 14 +: 1
                            __field ff 13 +: 1
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, U, ff, Zt) of
                                when ('00', _, _, '1xxxx') => __UNALLOCATED
                                when ('01', '0', '0', _) => __encoding LD1SH_Z.P.BZ_D.64.scaled // ld1sh_z_p_bz_d_64_scaled
                                when ('01', '0', '1', _) => __encoding LDFF1SH_Z.P.BZ_D.64.scaled // ldff1sh_z_p_bz_d_64_scaled
                                when ('01', '1', '0', _) => __encoding LD1H_Z.P.BZ_D.64.scaled // ld1h_z_p_bz_d_64_scaled
                                when ('01', '1', '1', _) => __encoding LDFF1H_Z.P.BZ_D.64.scaled // ldff1h_z_p_bz_d_64_scaled
                                when ('10', '0', '0', _) => __encoding LD1SW_Z.P.BZ_D.64.scaled // ld1sw_z_p_bz_d_64_scaled
                                when ('10', '0', '1', _) => __encoding LDFF1SW_Z.P.BZ_D.64.scaled // ldff1sw_z_p_bz_d_64_scaled
                                when ('10', '1', '0', _) => __encoding LD1W_Z.P.BZ_D.64.scaled // ld1w_z_p_bz_d_64_scaled
                                when ('10', '1', '1', _) => __encoding LDFF1W_Z.P.BZ_D.64.scaled // ldff1w_z_p_bz_d_64_scaled
                                when ('11', '0', _, _) => __UNALLOCATED
                                when ('11', '1', '0', _) => __encoding LD1D_Z.P.BZ_D.64.scaled // ld1d_z_p_bz_d_64_scaled
                                when ('11', '1', '1', _) => __encoding LDFF1D_Z.P.BZ_D.64.scaled // ldff1d_z_p_bz_d_64_scaled
                        when (_, _, 'x0', _, '0xx', _, _, _) => // sve_mem_64b_gld_vs
                            __field msz 23 +: 2
                            __field xs 22 +: 1
                            __field Zm 16 +: 5
                            __field U 14 +: 1
                            __field ff 13 +: 1
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, U, ff) of
                                when ('00', '0', '0') => __encoding LD1SB_Z.P.BZ_D.x32.unscaled // ld1sb_z_p_bz_d_x32_unscaled
                                when ('00', '0', '1') => __encoding LDFF1SB_Z.P.BZ_D.x32.unscaled // ldff1sb_z_p_bz_d_x32_unscaled
                                when ('00', '1', '0') => __encoding LD1B_Z.P.BZ_D.x32.unscaled // ld1b_z_p_bz_d_x32_unscaled
                                when ('00', '1', '1') => __encoding LDFF1B_Z.P.BZ_D.x32.unscaled // ldff1b_z_p_bz_d_x32_unscaled
                                when ('01', '0', '0') => __encoding LD1SH_Z.P.BZ_D.x32.unscaled // ld1sh_z_p_bz_d_x32_unscaled
                                when ('01', '0', '1') => __encoding LDFF1SH_Z.P.BZ_D.x32.unscaled // ldff1sh_z_p_bz_d_x32_unscaled
                                when ('01', '1', '0') => __encoding LD1H_Z.P.BZ_D.x32.unscaled // ld1h_z_p_bz_d_x32_unscaled
                                when ('01', '1', '1') => __encoding LDFF1H_Z.P.BZ_D.x32.unscaled // ldff1h_z_p_bz_d_x32_unscaled
                                when ('10', '0', '0') => __encoding LD1SW_Z.P.BZ_D.x32.unscaled // ld1sw_z_p_bz_d_x32_unscaled
                                when ('10', '0', '1') => __encoding LDFF1SW_Z.P.BZ_D.x32.unscaled // ldff1sw_z_p_bz_d_x32_unscaled
                                when ('10', '1', '0') => __encoding LD1W_Z.P.BZ_D.x32.unscaled // ld1w_z_p_bz_d_x32_unscaled
                                when ('10', '1', '1') => __encoding LDFF1W_Z.P.BZ_D.x32.unscaled // ldff1w_z_p_bz_d_x32_unscaled
                                when ('11', '0', _) => __UNALLOCATED
                                when ('11', '1', '0') => __encoding LD1D_Z.P.BZ_D.x32.unscaled // ld1d_z_p_bz_d_x32_unscaled
                                when ('11', '1', '1') => __encoding LDFF1D_Z.P.BZ_D.x32.unscaled // ldff1d_z_p_bz_d_x32_unscaled
                        when (_, _, 'x1', _, '0xx', _, _, _) => // sve_mem_64b_gld_sv
                            __field msz 23 +: 2
                            __field xs 22 +: 1
                            __field Zm 16 +: 5
                            __field U 14 +: 1
                            __field ff 13 +: 1
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, U, ff, Zt) of
                                when ('00', _, _, '1xxxx') => __UNALLOCATED
                                when ('01', '0', '0', _) => __encoding LD1SH_Z.P.BZ_D.x32.scaled // ld1sh_z_p_bz_d_x32_scaled
                                when ('01', '0', '1', _) => __encoding LDFF1SH_Z.P.BZ_D.x32.scaled // ldff1sh_z_p_bz_d_x32_scaled
                                when ('01', '1', '0', _) => __encoding LD1H_Z.P.BZ_D.x32.scaled // ld1h_z_p_bz_d_x32_scaled
                                when ('01', '1', '1', _) => __encoding LDFF1H_Z.P.BZ_D.x32.scaled // ldff1h_z_p_bz_d_x32_scaled
                                when ('10', '0', '0', _) => __encoding LD1SW_Z.P.BZ_D.x32.scaled // ld1sw_z_p_bz_d_x32_scaled
                                when ('10', '0', '1', _) => __encoding LDFF1SW_Z.P.BZ_D.x32.scaled // ldff1sw_z_p_bz_d_x32_scaled
                                when ('10', '1', '0', _) => __encoding LD1W_Z.P.BZ_D.x32.scaled // ld1w_z_p_bz_d_x32_scaled
                                when ('10', '1', '1', _) => __encoding LDFF1W_Z.P.BZ_D.x32.scaled // ldff1w_z_p_bz_d_x32_scaled
                                when ('11', '0', _, _) => __UNALLOCATED
                                when ('11', '1', '0', _) => __encoding LD1D_Z.P.BZ_D.x32.scaled // ld1d_z_p_bz_d_x32_scaled
                                when ('11', '1', '1', _) => __encoding LDFF1D_Z.P.BZ_D.x32.scaled // ldff1d_z_p_bz_d_x32_scaled
                when ('111', _, _, _, _, _, _, _) =>
                    // sve_memst
                    case (25 +: 7, 20 +: 5, 16 +: 4, 13 +: 3, 5 +: 8, 4 +: 1, 0 +: 4) of
                        when (_, '0xxxx', _, '00x', _, _, _) => __UNPREDICTABLE
                        when (_, '10xxx', _, '00x', _, _, _) => __UNPREDICTABLE
                        when (_, '110xx', _, '000', _, '0', _) => // sve_mem_pspill
                            __field imm9h 16 +: 6
                            __field imm9l 10 +: 3
                            __field Rn 5 +: 5
                            __field Pt 0 +: 4
                            case () of
                                when () => __encoding STR_P.BI__ // str_p_bi_
                        when (_, '110xx', _, '000', _, '1', _) => __UNPREDICTABLE
                        when (_, '110xx', _, '001', _, _, _) => __UNPREDICTABLE
                        when (_, '111xx', _, '00x', _, _, _) => __UNPREDICTABLE
                        when (_, 'xx00x', _, '101', _, _, _) => // sve_mem_sst_vs2
                            __field msz 23 +: 2
                            __field Zm 16 +: 5
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz) of
                                when ('00') => __encoding ST1B_Z.P.BZ_D.64.unscaled // st1b_z_p_bz_d_64_unscaled
                                when ('01') => __encoding ST1H_Z.P.BZ_D.64.unscaled // st1h_z_p_bz_d_64_unscaled
                                when ('10') => __encoding ST1W_Z.P.BZ_D.64.unscaled // st1w_z_p_bz_d_64_unscaled
                                when ('11') => __encoding ST1D_Z.P.BZ_D.64.unscaled // st1d_z_p_bz_d_64_unscaled
                        when (_, 'xx00x', _, '1x0', _, _, _) => // sve_mem_sst_vs_a
                            __field msz 23 +: 2
                            __field Zm 16 +: 5
                            __field xs 14 +: 1
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz) of
                                when ('00') => __encoding ST1B_Z.P.BZ_D.x32.unscaled // st1b_z_p_bz_d_x32_unscaled
                                when ('01') => __encoding ST1H_Z.P.BZ_D.x32.unscaled // st1h_z_p_bz_d_x32_unscaled
                                when ('10') => __encoding ST1W_Z.P.BZ_D.x32.unscaled // st1w_z_p_bz_d_x32_unscaled
                                when ('11') => __encoding ST1D_Z.P.BZ_D.x32.unscaled // st1d_z_p_bz_d_x32_unscaled
                        when (_, 'xx01x', _, '101', _, _, _) => // sve_mem_sst_sv2
                            __field msz 23 +: 2
                            __field Zm 16 +: 5
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz) of
                                when ('00') => __UNALLOCATED
                                when ('01') => __encoding ST1H_Z.P.BZ_D.64.scaled // st1h_z_p_bz_d_64_scaled
                                when ('10') => __encoding ST1W_Z.P.BZ_D.64.scaled // st1w_z_p_bz_d_64_scaled
                                when ('11') => __encoding ST1D_Z.P.BZ_D.64.scaled // st1d_z_p_bz_d_64_scaled
                        when (_, 'xx01x', _, '1x0', _, _, _) => // sve_mem_sst_sv_a
                            __field msz 23 +: 2
                            __field Zm 16 +: 5
                            __field xs 14 +: 1
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz) of
                                when ('00') => __UNALLOCATED
                                when ('01') => __encoding ST1H_Z.P.BZ_D.x32.scaled // st1h_z_p_bz_d_x32_scaled
                                when ('10') => __encoding ST1W_Z.P.BZ_D.x32.scaled // st1w_z_p_bz_d_x32_scaled
                                when ('11') => __encoding ST1D_Z.P.BZ_D.x32.scaled // st1d_z_p_bz_d_x32_scaled
                        when (_, 'xx10x', _, '101', _, _, _) => // sve_mem_sst_vi_a
                            __field msz 23 +: 2
                            __field imm5 16 +: 5
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz) of
                                when ('00') => __encoding ST1B_Z.P.AI_D // st1b_z_p_ai_d
                                when ('01') => __encoding ST1H_Z.P.AI_D // st1h_z_p_ai_d
                                when ('10') => __encoding ST1W_Z.P.AI_D // st1w_z_p_ai_d
                                when ('11') => __encoding ST1D_Z.P.AI_D // st1d_z_p_ai_d
                        when (_, 'xx10x', _, '1x0', _, _, _) => // sve_mem_sst_vs_b
                            __field msz 23 +: 2
                            __field Zm 16 +: 5
                            __field xs 14 +: 1
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz) of
                                when ('00') => __encoding ST1B_Z.P.BZ_S.x32.unscaled // st1b_z_p_bz_s_x32_unscaled
                                when ('01') => __encoding ST1H_Z.P.BZ_S.x32.unscaled // st1h_z_p_bz_s_x32_unscaled
                                when ('10') => __encoding ST1W_Z.P.BZ_S.x32.unscaled // st1w_z_p_bz_s_x32_unscaled
                                when ('11') => __UNALLOCATED
                        when (_, 'xx11x', _, '101', _, _, _) => // sve_mem_sst_vi_b
                            __field msz 23 +: 2
                            __field imm5 16 +: 5
                            __field Pg 10 +: 3
                            __field Zn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz) of
                                when ('00') => __encoding ST1B_Z.P.AI_S // st1b_z_p_ai_s
                                when ('01') => __encoding ST1H_Z.P.AI_S // st1h_z_p_ai_s
                                when ('10') => __encoding ST1W_Z.P.AI_S // st1w_z_p_ai_s
                                when ('11') => __UNALLOCATED
                        when (_, 'xx11x', _, '1x0', _, _, _) => // sve_mem_sst_sv_b
                            __field msz 23 +: 2
                            __field Zm 16 +: 5
                            __field xs 14 +: 1
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz) of
                                when ('00') => __UNALLOCATED
                                when ('01') => __encoding ST1H_Z.P.BZ_S.x32.scaled // st1h_z_p_bz_s_x32_scaled
                                when ('10') => __encoding ST1W_Z.P.BZ_S.x32.scaled // st1w_z_p_bz_s_x32_scaled
                                when ('11') => __UNALLOCATED
                        when (_, 'xxxx0', _, '111', _, _, _) => // sve_mem_cst_si
                            __field msz 23 +: 2
                            __field size 21 +: 2
                            __field imm4 16 +: 4
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz) of
                                when ('00') => __encoding ST1B_Z.P.BI__ // st1b_z_p_bi_
                                when ('01') => __encoding ST1H_Z.P.BI__ // st1h_z_p_bi_
                                when ('10') => __encoding ST1W_Z.P.BI__ // st1w_z_p_bi_
                                when ('11') => __encoding ST1D_Z.P.BI__ // st1d_z_p_bi_
                        when (_, 'xxxx1', _, '111', _, _, _) => // sve_mem_est_si
                            __field msz 23 +: 2
                            __field num 21 +: 2
                            __field imm4 16 +: 4
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, num) of
                                when ('00', '01') => __encoding ST2B_Z.P.BI_Contiguous // st2b_z_p_bi_contiguous
                                when ('00', '10') => __encoding ST3B_Z.P.BI_Contiguous // st3b_z_p_bi_contiguous
                                when ('00', '11') => __encoding ST4B_Z.P.BI_Contiguous // st4b_z_p_bi_contiguous
                                when ('01', '01') => __encoding ST2H_Z.P.BI_Contiguous // st2h_z_p_bi_contiguous
                                when ('01', '10') => __encoding ST3H_Z.P.BI_Contiguous // st3h_z_p_bi_contiguous
                                when ('01', '11') => __encoding ST4H_Z.P.BI_Contiguous // st4h_z_p_bi_contiguous
                                when ('10', '01') => __encoding ST2W_Z.P.BI_Contiguous // st2w_z_p_bi_contiguous
                                when ('10', '10') => __encoding ST3W_Z.P.BI_Contiguous // st3w_z_p_bi_contiguous
                                when ('10', '11') => __encoding ST4W_Z.P.BI_Contiguous // st4w_z_p_bi_contiguous
                                when ('11', '01') => __encoding ST2D_Z.P.BI_Contiguous // st2d_z_p_bi_contiguous
                                when ('11', '10') => __encoding ST3D_Z.P.BI_Contiguous // st3d_z_p_bi_contiguous
                                when ('11', '11') => __encoding ST4D_Z.P.BI_Contiguous // st4d_z_p_bi_contiguous
                        when (_, _, _, '010', _, _, _) => // sve_mem_cst_ss
                            __field msz 23 +: 2
                            __field size 21 +: 2
                            __field Rm 16 +: 5
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz) of
                                when ('00') => __encoding ST1B_Z.P.BR__ // st1b_z_p_br_
                                when ('01') => __encoding ST1H_Z.P.BR__ // st1h_z_p_br_
                                when ('10') => __encoding ST1W_Z.P.BR__ // st1w_z_p_br_
                                when ('11') => __encoding ST1D_Z.P.BR__ // st1d_z_p_br_
                        when (_, _, _, '011', _, _, _) => // sve_mem_est_ss
                            __field msz 23 +: 2
                            __field num 21 +: 2
                            __field Rm 16 +: 5
                            __field Pg 10 +: 3
                            __field Rn 5 +: 5
                            __field Zt 0 +: 5
                            case (msz, num) of
                                when ('00', '01') => __encoding ST2B_Z.P.BR_Contiguous // st2b_z_p_br_contiguous
                                when ('00', '10') => __encoding ST3B_Z.P.BR_Contiguous // st3b_z_p_br_contiguous
                                when ('00', '11') => __encoding ST4B_Z.P.BR_Contiguous // st4b_z_p_br_contiguous
                                when ('01', '01') => __encoding ST2H_Z.P.BR_Contiguous // st2h_z_p_br_contiguous
                                when ('01', '10') => __encoding ST3H_Z.P.BR_Contiguous // st3h_z_p_br_contiguous
                                when ('01', '11') => __encoding ST4H_Z.P.BR_Contiguous // st4h_z_p_br_contiguous
                                when ('10', '01') => __encoding ST2W_Z.P.BR_Contiguous // st2w_z_p_br_contiguous
                                when ('10', '10') => __encoding ST3W_Z.P.BR_Contiguous // st3w_z_p_br_contiguous
                                when ('10', '11') => __encoding ST4W_Z.P.BR_Contiguous // st4w_z_p_br_contiguous
                                when ('11', '01') => __encoding ST2D_Z.P.BR_Contiguous // st2d_z_p_br_contiguous
                                when ('11', '10') => __encoding ST3D_Z.P.BR_Contiguous // st3d_z_p_br_contiguous
                                when ('11', '11') => __encoding ST4D_Z.P.BR_Contiguous // st4d_z_p_br_contiguous
        when (_, '0011', _) => __UNPREDICTABLE
        when (_, '100x', _) =>
            // dpimm
            case (29 +: 3, 26 +: 3, 23 +: 3, 0 +: 23) of
                when (_, _, '00x', _) => // pcreladdr
                    __field op 31 +: 1
                    __field immlo 29 +: 2
                    __field immhi 5 +: 19
                    __field Rd 0 +: 5
                    case (op) of
                        when ('0') => __encoding aarch64_integer_arithmetic_address_pc_rel // ADR_only_pcreladdr
                        when ('1') => __encoding aarch64_integer_arithmetic_address_pc_rel // ADRP_only_pcreladdr
                when (_, _, '010', _) => // addsub_imm
                    __field sf 31 +: 1
                    __field op 30 +: 1
                    __field S 29 +: 1
                    __field sh 22 +: 1
                    __field imm12 10 +: 12
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, op, S) of
                        when ('0', '0', '0') => __encoding aarch64_integer_arithmetic_add_sub_immediate // ADD_32_addsub_imm
                        when ('0', '0', '1') => __encoding aarch64_integer_arithmetic_add_sub_immediate // ADDS_32S_addsub_imm
                        when ('0', '1', '0') => __encoding aarch64_integer_arithmetic_add_sub_immediate // SUB_32_addsub_imm
                        when ('0', '1', '1') => __encoding aarch64_integer_arithmetic_add_sub_immediate // SUBS_32S_addsub_imm
                        when ('1', '0', '0') => __encoding aarch64_integer_arithmetic_add_sub_immediate // ADD_64_addsub_imm
                        when ('1', '0', '1') => __encoding aarch64_integer_arithmetic_add_sub_immediate // ADDS_64S_addsub_imm
                        when ('1', '1', '0') => __encoding aarch64_integer_arithmetic_add_sub_immediate // SUB_64_addsub_imm
                        when ('1', '1', '1') => __encoding aarch64_integer_arithmetic_add_sub_immediate // SUBS_64S_addsub_imm
                when (_, _, '011', _) => // addsub_immtags
                    __field sf 31 +: 1
                    __field op 30 +: 1
                    __field S 29 +: 1
                    __field o2 22 +: 1
                    __field uimm6 16 +: 6
                    __field op3 14 +: 2
                    __field uimm4 10 +: 4
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, op, S) of
                        when ('0', _, _) => __UNALLOCATED
                        when ('1', _, '1') => __UNALLOCATED
                        when ('1', '0', '0') => __encoding aarch64_integer_tags_mcaddtag // ADDG_64_addsub_immtags
                        when ('1', '1', '0') => __encoding aarch64_integer_tags_mcsubtag // SUBG_64_addsub_immtags
                when (_, _, '100', _) => // log_imm
                    __field sf 31 +: 1
                    __field opc 29 +: 2
                    __field N 22 +: 1
                    __field immr 16 +: 6
                    __field imms 10 +: 6
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, opc, N) of
                        when ('0', _, '1') => __UNALLOCATED
                        when ('0', '00', '0') => __encoding aarch64_integer_logical_immediate // AND_32_log_imm
                        when ('0', '01', '0') => __encoding aarch64_integer_logical_immediate // ORR_32_log_imm
                        when ('0', '10', '0') => __encoding aarch64_integer_logical_immediate // EOR_32_log_imm
                        when ('0', '11', '0') => __encoding aarch64_integer_logical_immediate // ANDS_32S_log_imm
                        when ('1', '00', _) => __encoding aarch64_integer_logical_immediate // AND_64_log_imm
                        when ('1', '01', _) => __encoding aarch64_integer_logical_immediate // ORR_64_log_imm
                        when ('1', '10', _) => __encoding aarch64_integer_logical_immediate // EOR_64_log_imm
                        when ('1', '11', _) => __encoding aarch64_integer_logical_immediate // ANDS_64S_log_imm
                when (_, _, '101', _) => // movewide
                    __field sf 31 +: 1
                    __field opc 29 +: 2
                    __field hw 21 +: 2
                    __field imm16 5 +: 16
                    __field Rd 0 +: 5
                    case (sf, opc, hw) of
                        when (_, '01', _) => __UNALLOCATED
                        when ('0', _, '1x') => __UNALLOCATED
                        when ('0', '00', _) => __encoding aarch64_integer_ins_ext_insert_movewide // MOVN_32_movewide
                        when ('0', '10', _) => __encoding aarch64_integer_ins_ext_insert_movewide // MOVZ_32_movewide
                        when ('0', '11', _) => __encoding aarch64_integer_ins_ext_insert_movewide // MOVK_32_movewide
                        when ('1', '00', _) => __encoding aarch64_integer_ins_ext_insert_movewide // MOVN_64_movewide
                        when ('1', '10', _) => __encoding aarch64_integer_ins_ext_insert_movewide // MOVZ_64_movewide
                        when ('1', '11', _) => __encoding aarch64_integer_ins_ext_insert_movewide // MOVK_64_movewide
                when (_, _, '110', _) => // bitfield
                    __field sf 31 +: 1
                    __field opc 29 +: 2
                    __field N 22 +: 1
                    __field immr 16 +: 6
                    __field imms 10 +: 6
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, opc, N) of
                        when (_, '11', _) => __UNALLOCATED
                        when ('0', _, '1') => __UNALLOCATED
                        when ('0', '00', '0') => __encoding aarch64_integer_bitfield // SBFM_32M_bitfield
                        when ('0', '01', '0') => __encoding aarch64_integer_bitfield // BFM_32M_bitfield
                        when ('0', '10', '0') => __encoding aarch64_integer_bitfield // UBFM_32M_bitfield
                        when ('1', _, '0') => __UNALLOCATED
                        when ('1', '00', '1') => __encoding aarch64_integer_bitfield // SBFM_64M_bitfield
                        when ('1', '01', '1') => __encoding aarch64_integer_bitfield // BFM_64M_bitfield
                        when ('1', '10', '1') => __encoding aarch64_integer_bitfield // UBFM_64M_bitfield
                when (_, _, '111', _) => // extract
                    __field sf 31 +: 1
                    __field op21 29 +: 2
                    __field N 22 +: 1
                    __field o0 21 +: 1
                    __field Rm 16 +: 5
                    __field imms 10 +: 6
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, op21, N, o0, imms) of
                        when (_, 'x1', _, _, _) => __UNALLOCATED
                        when (_, '00', _, '1', _) => __UNALLOCATED
                        when (_, '1x', _, _, _) => __UNALLOCATED
                        when ('0', _, _, _, '1xxxxx') => __UNALLOCATED
                        when ('0', _, '1', _, _) => __UNALLOCATED
                        when ('0', '00', '0', '0', '0xxxxx') => __encoding aarch64_integer_ins_ext_extract_immediate // EXTR_32_extract
                        when ('1', _, '0', _, _) => __UNALLOCATED
                        when ('1', '00', '1', '0', _) => __encoding aarch64_integer_ins_ext_extract_immediate // EXTR_64_extract
        when (_, '101x', _) =>
            // control
            case (29 +: 3, 26 +: 3, 12 +: 14, 5 +: 7, 0 +: 5) of
                when ('010', _, '0xxxxxxxxxxxxx', _, _) => // condbranch
                    __field o1 24 +: 1
                    __field imm19 5 +: 19
                    __field o0 4 +: 1
                    __field cond 0 +: 4
                    case (o1, o0) of
                        when ('0', '0') => __encoding aarch64_branch_conditional_cond // B_only_condbranch
                        when ('0', '1') => __UNALLOCATED
                        when ('1', _) => __UNALLOCATED
                when ('110', _, '00xxxxxxxxxxxx', _, _) => // exception
                    __field opc 21 +: 3
                    __field imm16 5 +: 16
                    __field op2 2 +: 3
                    __field LL 0 +: 2
                    case (opc, op2, LL) of
                        when (_, 'xx1', _) => __UNALLOCATED
                        when (_, 'x1x', _) => __UNALLOCATED
                        when (_, '1xx', _) => __UNALLOCATED
                        when ('000', '000', '00') => __UNALLOCATED
                        when ('000', '000', '01') => __encoding aarch64_system_exceptions_runtime_svc // SVC_EX_exception
                        when ('000', '000', '10') => __encoding aarch64_system_exceptions_runtime_hvc // HVC_EX_exception
                        when ('000', '000', '11') => __encoding aarch64_system_exceptions_runtime_smc // SMC_EX_exception
                        when ('001', '000', 'x1') => __UNALLOCATED
                        when ('001', '000', '00') => __encoding aarch64_system_exceptions_debug_breakpoint // BRK_EX_exception
                        when ('001', '000', '1x') => __UNALLOCATED
                        when ('010', '000', 'x1') => __UNALLOCATED
                        when ('010', '000', '00') => __encoding aarch64_system_exceptions_debug_halt // HLT_EX_exception
                        when ('010', '000', '1x') => __UNALLOCATED
                        when ('011', '000', '01') => __UNALLOCATED
                        when ('011', '000', '1x') => __UNALLOCATED
                        when ('100', '000', '00') => __UNALLOCATED
                        when ('101', '000', '00') => __UNALLOCATED
                        when ('101', '000', '01') => __encoding aarch64_system_exceptions_debug_exception // DCPS1_DC_exception
                        when ('101', '000', '10') => __encoding aarch64_system_exceptions_debug_exception // DCPS2_DC_exception
                        when ('101', '000', '11') => __encoding aarch64_system_exceptions_debug_exception // DCPS3_DC_exception
                        when ('110', '000', _) => __UNALLOCATED
                        when ('111', '000', '01') => __UNALLOCATED
                        when ('111', '000', '1x') => __UNALLOCATED
                when ('110', _, '01000000110010', _, '11111') => // hints
                    __field CRm 8 +: 4
                    __field op2 5 +: 3
                    case (CRm, op2) of
                        when (_, _) => __encoding aarch64_system_hints // HINT_HM_hints
                        when ('0000', '000') => __encoding aarch64_system_hints // NOP_HI_hints
                        when ('0000', '001') => __encoding aarch64_system_hints // YIELD_HI_hints
                        when ('0000', '010') => __encoding aarch64_system_hints // WFE_HI_hints
                        when ('0000', '011') => __encoding aarch64_system_hints // WFI_HI_hints
                        when ('0000', '100') => __encoding aarch64_system_hints // SEV_HI_hints
                        when ('0000', '101') => __encoding aarch64_system_hints // SEVL_HI_hints
                        when ('0000', '111') => __encoding aarch64_integer_pac_strip_hint // XPACLRI_HI_hints
                        when ('0001', '000') => __encoding aarch64_integer_pac_pacia_hint // PACIA1716_HI_hints
                        when ('0001', '010') => __encoding aarch64_integer_pac_pacib_hint // PACIB1716_HI_hints
                        when ('0001', '100') => __encoding aarch64_integer_pac_autia_hint // AUTIA1716_HI_hints
                        when ('0001', '110') => __encoding aarch64_integer_pac_autib_hint // AUTIB1716_HI_hints
                        when ('0010', '000') => __encoding aarch64_system_hints // ESB_HI_hints
                        when ('0010', '001') => __encoding aarch64_system_hints // PSB_HC_hints
                        when ('0010', '010') => __encoding aarch64_system_hints // TSB_HC_hints
                        when ('0010', '100') => __encoding aarch64_system_hints // CSDB_HI_hints
                        when ('0011', '000') => __encoding aarch64_integer_pac_pacia_hint // PACIAZ_HI_hints
                        when ('0011', '001') => __encoding aarch64_integer_pac_pacia_hint // PACIASP_HI_hints
                        when ('0011', '010') => __encoding aarch64_integer_pac_pacib_hint // PACIBZ_HI_hints
                        when ('0011', '011') => __encoding aarch64_integer_pac_pacib_hint // PACIBSP_HI_hints
                        when ('0011', '100') => __encoding aarch64_integer_pac_autia_hint // AUTIAZ_HI_hints
                        when ('0011', '101') => __encoding aarch64_integer_pac_autia_hint // AUTIASP_HI_hints
                        when ('0011', '110') => __encoding aarch64_integer_pac_autib_hint // AUTIBZ_HI_hints
                        when ('0011', '111') => __encoding aarch64_integer_pac_autib_hint // AUTIBSP_HI_hints
                        when ('0100', 'xx0') => __encoding aarch64_system_hints // BTI_HB_hints
                when ('110', _, '01000000110011', _, _) => // barriers
                    __field CRm 8 +: 4
                    __field op2 5 +: 3
                    __field Rt 0 +: 5
                    case (CRm, op2, Rt) of
                        when (_, '000', _) => __UNALLOCATED
                        when (_, '001', _) => __UNALLOCATED
                        when (_, '010', '11111') => __encoding aarch64_system_monitors // CLREX_BN_barriers
                        when (_, '101', '11111') => __encoding aarch64_system_barriers // DMB_BO_barriers
                        when (_, '110', '11111') => __encoding aarch64_system_barriers // ISB_BI_barriers
                        when (_, '111', !'11111') => __UNALLOCATED
                        when (_, '111', '11111') => __encoding aarch64_system_barriers // SB_only_barriers
                        when (!'0x00', '100', '11111') => __encoding aarch64_system_barriers // DSB_BO_barriers
                        when ('0000', '100', '11111') => __encoding aarch64_system_barriers // SSBB_only_barriers
                        when ('0001', '011', _) => __UNALLOCATED
                        when ('001x', '011', _) => __UNALLOCATED
                        when ('01xx', '011', _) => __UNALLOCATED
                        when ('0100', '100', '11111') => __encoding aarch64_system_barriers // PSSBB_only_barriers
                        when ('1xxx', '011', _) => __UNALLOCATED
                when ('110', _, '0100000xxx0100', _, _) => // pstate
                    __field op1 16 +: 3
                    __field CRm 8 +: 4
                    __field op2 5 +: 3
                    __field Rt 0 +: 5
                    case (op1, op2, Rt) of
                        when (_, _, !'11111') => __UNALLOCATED
                        when (_, _, '11111') => __encoding aarch64_system_register_cpsr // MSR_SI_pstate
                        when ('000', '000', '11111') => __encoding aarch64_integer_flags_cfinv // CFINV_M_pstate
                        when ('000', '001', '11111') => __encoding aarch64_integer_flags_xaflag // XAFlag_M_pstate
                        when ('000', '010', '11111') => __encoding aarch64_integer_flags_axflag // AXFlag_M_pstate
                when ('110', _, '0100x01xxxxxxx', _, _) => // systeminstrs
                    __field L 21 +: 1
                    __field op1 16 +: 3
                    __field CRn 12 +: 4
                    __field CRm 8 +: 4
                    __field op2 5 +: 3
                    __field Rt 0 +: 5
                    case (L) of
                        when ('0') => __encoding aarch64_system_sysops // SYS_CR_systeminstrs
                        when ('1') => __encoding aarch64_system_sysops // SYSL_RC_systeminstrs
                when ('110', _, '0100x1xxxxxxxx', _, _) => // systemmove
                    __field L 21 +: 1
                    __field o0 19 +: 1
                    __field op1 16 +: 3
                    __field CRn 12 +: 4
                    __field CRm 8 +: 4
                    __field op2 5 +: 3
                    __field Rt 0 +: 5
                    case (L) of
                        when ('0') => __encoding aarch64_system_register_system // MSR_SR_systemmove
                        when ('1') => __encoding aarch64_system_register_system // MRS_RS_systemmove
                when ('110', _, '1xxxxxxxxxxxxx', _, _) => // branch_reg
                    __field opc 21 +: 4
                    __field op2 16 +: 5
                    __field op3 10 +: 6
                    __field Rn 5 +: 5
                    __field op4 0 +: 5
                    case (opc, op2, op3, Rn, op4) of
                        when (_, !'11111', _, _, _) => __UNALLOCATED
                        when ('0000', '11111', '000000', _, !'00000') => __UNALLOCATED
                        when ('0000', '11111', '000000', _, '00000') => __encoding aarch64_branch_unconditional_register // BR_64_branch_reg
                        when ('0000', '11111', '000001', _, _) => __UNALLOCATED
                        when ('0000', '11111', '000010', _, !'11111') => __UNALLOCATED
                        when ('0000', '11111', '000010', _, '11111') => __encoding aarch64_branch_unconditional_register // BRAAZ_64_branch_reg
                        when ('0000', '11111', '000011', _, !'11111') => __UNALLOCATED
                        when ('0000', '11111', '000011', _, '11111') => __encoding aarch64_branch_unconditional_register // BRABZ_64_branch_reg
                        when ('0000', '11111', '0001xx', _, _) => __UNALLOCATED
                        when ('0000', '11111', '001xxx', _, _) => __UNALLOCATED
                        when ('0000', '11111', '01xxxx', _, _) => __UNALLOCATED
                        when ('0000', '11111', '1xxxxx', _, _) => __UNALLOCATED
                        when ('0001', '11111', '000000', _, !'00000') => __UNALLOCATED
                        when ('0001', '11111', '000000', _, '00000') => __encoding aarch64_branch_unconditional_register // BLR_64_branch_reg
                        when ('0001', '11111', '000001', _, _) => __UNALLOCATED
                        when ('0001', '11111', '000010', _, !'11111') => __UNALLOCATED
                        when ('0001', '11111', '000010', _, '11111') => __encoding aarch64_branch_unconditional_register // BLRAAZ_64_branch_reg
                        when ('0001', '11111', '000011', _, !'11111') => __UNALLOCATED
                        when ('0001', '11111', '000011', _, '11111') => __encoding aarch64_branch_unconditional_register // BLRABZ_64_branch_reg
                        when ('0001', '11111', '0001xx', _, _) => __UNALLOCATED
                        when ('0001', '11111', '001xxx', _, _) => __UNALLOCATED
                        when ('0001', '11111', '01xxxx', _, _) => __UNALLOCATED
                        when ('0001', '11111', '1xxxxx', _, _) => __UNALLOCATED
                        when ('0010', '11111', '000000', _, !'00000') => __UNALLOCATED
                        when ('0010', '11111', '000000', _, '00000') => __encoding aarch64_branch_unconditional_register // RET_64R_branch_reg
                        when ('0010', '11111', '000001', _, _) => __UNALLOCATED
                        when ('0010', '11111', '000010', !'11111', !'11111') => __UNALLOCATED
                        when ('0010', '11111', '000010', '11111', '11111') => __encoding aarch64_branch_unconditional_register // RETAA_64E_branch_reg
                        when ('0010', '11111', '000011', !'11111', !'11111') => __UNALLOCATED
                        when ('0010', '11111', '000011', '11111', '11111') => __encoding aarch64_branch_unconditional_register // RETAB_64E_branch_reg
                        when ('0010', '11111', '0001xx', _, _) => __UNALLOCATED
                        when ('0010', '11111', '001xxx', _, _) => __UNALLOCATED
                        when ('0010', '11111', '01xxxx', _, _) => __UNALLOCATED
                        when ('0010', '11111', '1xxxxx', _, _) => __UNALLOCATED
                        when ('0011', '11111', _, _, _) => __UNALLOCATED
                        when ('0100', '11111', '000000', !'11111', !'00000') => __UNALLOCATED
                        when ('0100', '11111', '000000', !'11111', '00000') => __UNALLOCATED
                        when ('0100', '11111', '000000', '11111', !'00000') => __UNALLOCATED
                        when ('0100', '11111', '000000', '11111', '00000') => __encoding aarch64_branch_unconditional_eret // ERET_64E_branch_reg
                        when ('0100', '11111', '000001', _, _) => __UNALLOCATED
                        when ('0100', '11111', '000010', !'11111', !'11111') => __UNALLOCATED
                        when ('0100', '11111', '000010', !'11111', '11111') => __UNALLOCATED
                        when ('0100', '11111', '000010', '11111', !'11111') => __UNALLOCATED
                        when ('0100', '11111', '000010', '11111', '11111') => __encoding aarch64_branch_unconditional_eret // ERETAA_64E_branch_reg
                        when ('0100', '11111', '000011', !'11111', !'11111') => __UNALLOCATED
                        when ('0100', '11111', '000011', !'11111', '11111') => __UNALLOCATED
                        when ('0100', '11111', '000011', '11111', !'11111') => __UNALLOCATED
                        when ('0100', '11111', '000011', '11111', '11111') => __encoding aarch64_branch_unconditional_eret // ERETAB_64E_branch_reg
                        when ('0100', '11111', '0001xx', _, _) => __UNALLOCATED
                        when ('0100', '11111', '001xxx', _, _) => __UNALLOCATED
                        when ('0100', '11111', '01xxxx', _, _) => __UNALLOCATED
                        when ('0100', '11111', '1xxxxx', _, _) => __UNALLOCATED
                        when ('0101', '11111', !'000000', _, _) => __UNALLOCATED
                        when ('0101', '11111', '000000', !'11111', !'00000') => __UNALLOCATED
                        when ('0101', '11111', '000000', !'11111', '00000') => __UNALLOCATED
                        when ('0101', '11111', '000000', '11111', !'00000') => __UNALLOCATED
                        when ('0101', '11111', '000000', '11111', '00000') => __encoding aarch64_branch_unconditional_dret // DRPS_64E_branch_reg
                        when ('011x', '11111', _, _, _) => __UNALLOCATED
                        when ('1000', '11111', '00000x', _, _) => __UNALLOCATED
                        when ('1000', '11111', '000010', _, _) => __encoding aarch64_branch_unconditional_register // BRAA_64P_branch_reg
                        when ('1000', '11111', '000011', _, _) => __encoding aarch64_branch_unconditional_register // BRAB_64P_branch_reg
                        when ('1000', '11111', '0001xx', _, _) => __UNALLOCATED
                        when ('1000', '11111', '001xxx', _, _) => __UNALLOCATED
                        when ('1000', '11111', '01xxxx', _, _) => __UNALLOCATED
                        when ('1000', '11111', '1xxxxx', _, _) => __UNALLOCATED
                        when ('1001', '11111', '00000x', _, _) => __UNALLOCATED
                        when ('1001', '11111', '000010', _, _) => __encoding aarch64_branch_unconditional_register // BLRAA_64P_branch_reg
                        when ('1001', '11111', '000011', _, _) => __encoding aarch64_branch_unconditional_register // BLRAB_64P_branch_reg
                        when ('1001', '11111', '0001xx', _, _) => __UNALLOCATED
                        when ('1001', '11111', '001xxx', _, _) => __UNALLOCATED
                        when ('1001', '11111', '01xxxx', _, _) => __UNALLOCATED
                        when ('1001', '11111', '1xxxxx', _, _) => __UNALLOCATED
                        when ('101x', '11111', _, _, _) => __UNALLOCATED
                        when ('11xx', '11111', _, _, _) => __UNALLOCATED
                when ('x00', _, _, _, _) => // branch_imm
                    __field op 31 +: 1
                    __field imm26 0 +: 26
                    case (op) of
                        when ('0') => __encoding aarch64_branch_unconditional_immediate // B_only_branch_imm
                        when ('1') => __encoding aarch64_branch_unconditional_immediate // BL_only_branch_imm
                when ('x01', _, '0xxxxxxxxxxxxx', _, _) => // compbranch
                    __field sf 31 +: 1
                    __field op 24 +: 1
                    __field imm19 5 +: 19
                    __field Rt 0 +: 5
                    case (sf, op) of
                        when ('0', '0') => __encoding aarch64_branch_conditional_compare // CBZ_32_compbranch
                        when ('0', '1') => __encoding aarch64_branch_conditional_compare // CBNZ_32_compbranch
                        when ('1', '0') => __encoding aarch64_branch_conditional_compare // CBZ_64_compbranch
                        when ('1', '1') => __encoding aarch64_branch_conditional_compare // CBNZ_64_compbranch
                when ('x01', _, '1xxxxxxxxxxxxx', _, _) => // testbranch
                    __field b5 31 +: 1
                    __field op 24 +: 1
                    __field b40 19 +: 5
                    __field imm14 5 +: 14
                    __field Rt 0 +: 5
                    case (op) of
                        when ('0') => __encoding aarch64_branch_conditional_test // TBZ_only_testbranch
                        when ('1') => __encoding aarch64_branch_conditional_test // TBNZ_only_testbranch
        when (_, 'x1x0', _) =>
            // ldst
            case (28 +: 4, 27 +: 1, 26 +: 1, 25 +: 1, 23 +: 2, 22 +: 1, 16 +: 6, 12 +: 4, 10 +: 2, 0 +: 10) of
                when ('0x00', _, '1', _, '00', _, '000000', _, _, _) => // asisdlse
                    __field Q 30 +: 1
                    __field L 22 +: 1
                    __field opcode 12 +: 4
                    __field size 10 +: 2
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (L, opcode) of
                        when ('0', '0000') => __encoding aarch64_memory_vector_multiple_no_wb // ST4_asisdlse_R4
                        when ('0', '0001') => __UNALLOCATED
                        when ('0', '0010') => __encoding aarch64_memory_vector_multiple_no_wb // ST1_asisdlse_R4_4v
                        when ('0', '0011') => __UNALLOCATED
                        when ('0', '0100') => __encoding aarch64_memory_vector_multiple_no_wb // ST3_asisdlse_R3
                        when ('0', '0101') => __UNALLOCATED
                        when ('0', '0110') => __encoding aarch64_memory_vector_multiple_no_wb // ST1_asisdlse_R3_3v
                        when ('0', '0111') => __encoding aarch64_memory_vector_multiple_no_wb // ST1_asisdlse_R1_1v
                        when ('0', '1000') => __encoding aarch64_memory_vector_multiple_no_wb // ST2_asisdlse_R2
                        when ('0', '1001') => __UNALLOCATED
                        when ('0', '1010') => __encoding aarch64_memory_vector_multiple_no_wb // ST1_asisdlse_R2_2v
                        when ('0', '1011') => __UNALLOCATED
                        when ('0', '11xx') => __UNALLOCATED
                        when ('1', '0000') => __encoding aarch64_memory_vector_multiple_no_wb // LD4_asisdlse_R4
                        when ('1', '0001') => __UNALLOCATED
                        when ('1', '0010') => __encoding aarch64_memory_vector_multiple_no_wb // LD1_asisdlse_R4_4v
                        when ('1', '0011') => __UNALLOCATED
                        when ('1', '0100') => __encoding aarch64_memory_vector_multiple_no_wb // LD3_asisdlse_R3
                        when ('1', '0101') => __UNALLOCATED
                        when ('1', '0110') => __encoding aarch64_memory_vector_multiple_no_wb // LD1_asisdlse_R3_3v
                        when ('1', '0111') => __encoding aarch64_memory_vector_multiple_no_wb // LD1_asisdlse_R1_1v
                        when ('1', '1000') => __encoding aarch64_memory_vector_multiple_no_wb // LD2_asisdlse_R2
                        when ('1', '1001') => __UNALLOCATED
                        when ('1', '1010') => __encoding aarch64_memory_vector_multiple_no_wb // LD1_asisdlse_R2_2v
                        when ('1', '1011') => __UNALLOCATED
                        when ('1', '11xx') => __UNALLOCATED
                when ('0x00', _, '1', _, '01', _, '0xxxxx', _, _, _) => // asisdlsep
                    __field Q 30 +: 1
                    __field L 22 +: 1
                    __field Rm 16 +: 5
                    __field opcode 12 +: 4
                    __field size 10 +: 2
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (L, Rm, opcode) of
                        when ('0', _, '0001') => __UNALLOCATED
                        when ('0', _, '0011') => __UNALLOCATED
                        when ('0', _, '0101') => __UNALLOCATED
                        when ('0', _, '1001') => __UNALLOCATED
                        when ('0', _, '1011') => __UNALLOCATED
                        when ('0', _, '11xx') => __UNALLOCATED
                        when ('0', !'11111', '0000') => __encoding aarch64_memory_vector_multiple_post_inc // ST4_asisdlsep_R4_r
                        when ('0', !'11111', '0010') => __encoding aarch64_memory_vector_multiple_post_inc // ST1_asisdlsep_R4_r4
                        when ('0', !'11111', '0100') => __encoding aarch64_memory_vector_multiple_post_inc // ST3_asisdlsep_R3_r
                        when ('0', !'11111', '0110') => __encoding aarch64_memory_vector_multiple_post_inc // ST1_asisdlsep_R3_r3
                        when ('0', !'11111', '0111') => __encoding aarch64_memory_vector_multiple_post_inc // ST1_asisdlsep_R1_r1
                        when ('0', !'11111', '1000') => __encoding aarch64_memory_vector_multiple_post_inc // ST2_asisdlsep_R2_r
                        when ('0', !'11111', '1010') => __encoding aarch64_memory_vector_multiple_post_inc // ST1_asisdlsep_R2_r2
                        when ('0', '11111', '0000') => __encoding aarch64_memory_vector_multiple_post_inc // ST4_asisdlsep_I4_i
                        when ('0', '11111', '0010') => __encoding aarch64_memory_vector_multiple_post_inc // ST1_asisdlsep_I4_i4
                        when ('0', '11111', '0100') => __encoding aarch64_memory_vector_multiple_post_inc // ST3_asisdlsep_I3_i
                        when ('0', '11111', '0110') => __encoding aarch64_memory_vector_multiple_post_inc // ST1_asisdlsep_I3_i3
                        when ('0', '11111', '0111') => __encoding aarch64_memory_vector_multiple_post_inc // ST1_asisdlsep_I1_i1
                        when ('0', '11111', '1000') => __encoding aarch64_memory_vector_multiple_post_inc // ST2_asisdlsep_I2_i
                        when ('0', '11111', '1010') => __encoding aarch64_memory_vector_multiple_post_inc // ST1_asisdlsep_I2_i2
                        when ('1', _, '0001') => __UNALLOCATED
                        when ('1', _, '0011') => __UNALLOCATED
                        when ('1', _, '0101') => __UNALLOCATED
                        when ('1', _, '1001') => __UNALLOCATED
                        when ('1', _, '1011') => __UNALLOCATED
                        when ('1', _, '11xx') => __UNALLOCATED
                        when ('1', !'11111', '0000') => __encoding aarch64_memory_vector_multiple_post_inc // LD4_asisdlsep_R4_r
                        when ('1', !'11111', '0010') => __encoding aarch64_memory_vector_multiple_post_inc // LD1_asisdlsep_R4_r4
                        when ('1', !'11111', '0100') => __encoding aarch64_memory_vector_multiple_post_inc // LD3_asisdlsep_R3_r
                        when ('1', !'11111', '0110') => __encoding aarch64_memory_vector_multiple_post_inc // LD1_asisdlsep_R3_r3
                        when ('1', !'11111', '0111') => __encoding aarch64_memory_vector_multiple_post_inc // LD1_asisdlsep_R1_r1
                        when ('1', !'11111', '1000') => __encoding aarch64_memory_vector_multiple_post_inc // LD2_asisdlsep_R2_r
                        when ('1', !'11111', '1010') => __encoding aarch64_memory_vector_multiple_post_inc // LD1_asisdlsep_R2_r2
                        when ('1', '11111', '0000') => __encoding aarch64_memory_vector_multiple_post_inc // LD4_asisdlsep_I4_i
                        when ('1', '11111', '0010') => __encoding aarch64_memory_vector_multiple_post_inc // LD1_asisdlsep_I4_i4
                        when ('1', '11111', '0100') => __encoding aarch64_memory_vector_multiple_post_inc // LD3_asisdlsep_I3_i
                        when ('1', '11111', '0110') => __encoding aarch64_memory_vector_multiple_post_inc // LD1_asisdlsep_I3_i3
                        when ('1', '11111', '0111') => __encoding aarch64_memory_vector_multiple_post_inc // LD1_asisdlsep_I1_i1
                        when ('1', '11111', '1000') => __encoding aarch64_memory_vector_multiple_post_inc // LD2_asisdlsep_I2_i
                        when ('1', '11111', '1010') => __encoding aarch64_memory_vector_multiple_post_inc // LD1_asisdlsep_I2_i2
                when ('0x00', _, '1', _, '0x', _, '1xxxxx', _, _, _) => __UNPREDICTABLE
                when ('0x00', _, '1', _, '10', _, 'x00000', _, _, _) => // asisdlso
                    __field Q 30 +: 1
                    __field L 22 +: 1
                    __field R 21 +: 1
                    __field opcode 13 +: 3
                    __field S 12 +: 1
                    __field size 10 +: 2
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (L, R, opcode, S, size) of
                        when ('0', _, '11x', _, _) => __UNALLOCATED
                        when ('0', '0', '000', _, _) => __encoding aarch64_memory_vector_single_no_wb // ST1_asisdlso_B1_1b
                        when ('0', '0', '001', _, _) => __encoding aarch64_memory_vector_single_no_wb // ST3_asisdlso_B3_3b
                        when ('0', '0', '010', _, 'x0') => __encoding aarch64_memory_vector_single_no_wb // ST1_asisdlso_H1_1h
                        when ('0', '0', '010', _, 'x1') => __UNALLOCATED
                        when ('0', '0', '011', _, 'x0') => __encoding aarch64_memory_vector_single_no_wb // ST3_asisdlso_H3_3h
                        when ('0', '0', '011', _, 'x1') => __UNALLOCATED
                        when ('0', '0', '100', _, '00') => __encoding aarch64_memory_vector_single_no_wb // ST1_asisdlso_S1_1s
                        when ('0', '0', '100', _, '1x') => __UNALLOCATED
                        when ('0', '0', '100', '0', '01') => __encoding aarch64_memory_vector_single_no_wb // ST1_asisdlso_D1_1d
                        when ('0', '0', '100', '1', '01') => __UNALLOCATED
                        when ('0', '0', '101', _, '00') => __encoding aarch64_memory_vector_single_no_wb // ST3_asisdlso_S3_3s
                        when ('0', '0', '101', _, '10') => __UNALLOCATED
                        when ('0', '0', '101', '0', '01') => __encoding aarch64_memory_vector_single_no_wb // ST3_asisdlso_D3_3d
                        when ('0', '0', '101', '0', '11') => __UNALLOCATED
                        when ('0', '0', '101', '1', 'x1') => __UNALLOCATED
                        when ('0', '1', '000', _, _) => __encoding aarch64_memory_vector_single_no_wb // ST2_asisdlso_B2_2b
                        when ('0', '1', '001', _, _) => __encoding aarch64_memory_vector_single_no_wb // ST4_asisdlso_B4_4b
                        when ('0', '1', '010', _, 'x0') => __encoding aarch64_memory_vector_single_no_wb // ST2_asisdlso_H2_2h
                        when ('0', '1', '010', _, 'x1') => __UNALLOCATED
                        when ('0', '1', '011', _, 'x0') => __encoding aarch64_memory_vector_single_no_wb // ST4_asisdlso_H4_4h
                        when ('0', '1', '011', _, 'x1') => __UNALLOCATED
                        when ('0', '1', '100', _, '00') => __encoding aarch64_memory_vector_single_no_wb // ST2_asisdlso_S2_2s
                        when ('0', '1', '100', _, '10') => __UNALLOCATED
                        when ('0', '1', '100', '0', '01') => __encoding aarch64_memory_vector_single_no_wb // ST2_asisdlso_D2_2d
                        when ('0', '1', '100', '0', '11') => __UNALLOCATED
                        when ('0', '1', '100', '1', 'x1') => __UNALLOCATED
                        when ('0', '1', '101', _, '00') => __encoding aarch64_memory_vector_single_no_wb // ST4_asisdlso_S4_4s
                        when ('0', '1', '101', _, '10') => __UNALLOCATED
                        when ('0', '1', '101', '0', '01') => __encoding aarch64_memory_vector_single_no_wb // ST4_asisdlso_D4_4d
                        when ('0', '1', '101', '0', '11') => __UNALLOCATED
                        when ('0', '1', '101', '1', 'x1') => __UNALLOCATED
                        when ('1', '0', '000', _, _) => __encoding aarch64_memory_vector_single_no_wb // LD1_asisdlso_B1_1b
                        when ('1', '0', '001', _, _) => __encoding aarch64_memory_vector_single_no_wb // LD3_asisdlso_B3_3b
                        when ('1', '0', '010', _, 'x0') => __encoding aarch64_memory_vector_single_no_wb // LD1_asisdlso_H1_1h
                        when ('1', '0', '010', _, 'x1') => __UNALLOCATED
                        when ('1', '0', '011', _, 'x0') => __encoding aarch64_memory_vector_single_no_wb // LD3_asisdlso_H3_3h
                        when ('1', '0', '011', _, 'x1') => __UNALLOCATED
                        when ('1', '0', '100', _, '00') => __encoding aarch64_memory_vector_single_no_wb // LD1_asisdlso_S1_1s
                        when ('1', '0', '100', _, '1x') => __UNALLOCATED
                        when ('1', '0', '100', '0', '01') => __encoding aarch64_memory_vector_single_no_wb // LD1_asisdlso_D1_1d
                        when ('1', '0', '100', '1', '01') => __UNALLOCATED
                        when ('1', '0', '101', _, '00') => __encoding aarch64_memory_vector_single_no_wb // LD3_asisdlso_S3_3s
                        when ('1', '0', '101', _, '10') => __UNALLOCATED
                        when ('1', '0', '101', '0', '01') => __encoding aarch64_memory_vector_single_no_wb // LD3_asisdlso_D3_3d
                        when ('1', '0', '101', '0', '11') => __UNALLOCATED
                        when ('1', '0', '101', '1', 'x1') => __UNALLOCATED
                        when ('1', '0', '110', '0', _) => __encoding aarch64_memory_vector_single_no_wb // LD1R_asisdlso_R1
                        when ('1', '0', '110', '1', _) => __UNALLOCATED
                        when ('1', '0', '111', '0', _) => __encoding aarch64_memory_vector_single_no_wb // LD3R_asisdlso_R3
                        when ('1', '0', '111', '1', _) => __UNALLOCATED
                        when ('1', '1', '000', _, _) => __encoding aarch64_memory_vector_single_no_wb // LD2_asisdlso_B2_2b
                        when ('1', '1', '001', _, _) => __encoding aarch64_memory_vector_single_no_wb // LD4_asisdlso_B4_4b
                        when ('1', '1', '010', _, 'x0') => __encoding aarch64_memory_vector_single_no_wb // LD2_asisdlso_H2_2h
                        when ('1', '1', '010', _, 'x1') => __UNALLOCATED
                        when ('1', '1', '011', _, 'x0') => __encoding aarch64_memory_vector_single_no_wb // LD4_asisdlso_H4_4h
                        when ('1', '1', '011', _, 'x1') => __UNALLOCATED
                        when ('1', '1', '100', _, '00') => __encoding aarch64_memory_vector_single_no_wb // LD2_asisdlso_S2_2s
                        when ('1', '1', '100', _, '10') => __UNALLOCATED
                        when ('1', '1', '100', '0', '01') => __encoding aarch64_memory_vector_single_no_wb // LD2_asisdlso_D2_2d
                        when ('1', '1', '100', '0', '11') => __UNALLOCATED
                        when ('1', '1', '100', '1', 'x1') => __UNALLOCATED
                        when ('1', '1', '101', _, '00') => __encoding aarch64_memory_vector_single_no_wb // LD4_asisdlso_S4_4s
                        when ('1', '1', '101', _, '10') => __UNALLOCATED
                        when ('1', '1', '101', '0', '01') => __encoding aarch64_memory_vector_single_no_wb // LD4_asisdlso_D4_4d
                        when ('1', '1', '101', '0', '11') => __UNALLOCATED
                        when ('1', '1', '101', '1', 'x1') => __UNALLOCATED
                        when ('1', '1', '110', '0', _) => __encoding aarch64_memory_vector_single_no_wb // LD2R_asisdlso_R2
                        when ('1', '1', '110', '1', _) => __UNALLOCATED
                        when ('1', '1', '111', '0', _) => __encoding aarch64_memory_vector_single_no_wb // LD4R_asisdlso_R4
                        when ('1', '1', '111', '1', _) => __UNALLOCATED
                when ('0x00', _, '1', _, '11', _, _, _, _, _) => // asisdlsop
                    __field Q 30 +: 1
                    __field L 22 +: 1
                    __field R 21 +: 1
                    __field Rm 16 +: 5
                    __field opcode 13 +: 3
                    __field S 12 +: 1
                    __field size 10 +: 2
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (L, R, Rm, opcode, S, size) of
                        when ('0', _, _, '11x', _, _) => __UNALLOCATED
                        when ('0', '0', _, '010', _, 'x1') => __UNALLOCATED
                        when ('0', '0', _, '011', _, 'x1') => __UNALLOCATED
                        when ('0', '0', _, '100', _, '1x') => __UNALLOCATED
                        when ('0', '0', _, '100', '1', '01') => __UNALLOCATED
                        when ('0', '0', _, '101', _, '10') => __UNALLOCATED
                        when ('0', '0', _, '101', '0', '11') => __UNALLOCATED
                        when ('0', '0', _, '101', '1', 'x1') => __UNALLOCATED
                        when ('0', '0', !'11111', '000', _, _) => __encoding aarch64_memory_vector_single_post_inc // ST1_asisdlsop_BX1_r1b
                        when ('0', '0', !'11111', '001', _, _) => __encoding aarch64_memory_vector_single_post_inc // ST3_asisdlsop_BX3_r3b
                        when ('0', '0', !'11111', '010', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // ST1_asisdlsop_HX1_r1h
                        when ('0', '0', !'11111', '011', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // ST3_asisdlsop_HX3_r3h
                        when ('0', '0', !'11111', '100', _, '00') => __encoding aarch64_memory_vector_single_post_inc // ST1_asisdlsop_SX1_r1s
                        when ('0', '0', !'11111', '100', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // ST1_asisdlsop_DX1_r1d
                        when ('0', '0', !'11111', '101', _, '00') => __encoding aarch64_memory_vector_single_post_inc // ST3_asisdlsop_SX3_r3s
                        when ('0', '0', !'11111', '101', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // ST3_asisdlsop_DX3_r3d
                        when ('0', '0', '11111', '000', _, _) => __encoding aarch64_memory_vector_single_post_inc // ST1_asisdlsop_B1_i1b
                        when ('0', '0', '11111', '001', _, _) => __encoding aarch64_memory_vector_single_post_inc // ST3_asisdlsop_B3_i3b
                        when ('0', '0', '11111', '010', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // ST1_asisdlsop_H1_i1h
                        when ('0', '0', '11111', '011', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // ST3_asisdlsop_H3_i3h
                        when ('0', '0', '11111', '100', _, '00') => __encoding aarch64_memory_vector_single_post_inc // ST1_asisdlsop_S1_i1s
                        when ('0', '0', '11111', '100', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // ST1_asisdlsop_D1_i1d
                        when ('0', '0', '11111', '101', _, '00') => __encoding aarch64_memory_vector_single_post_inc // ST3_asisdlsop_S3_i3s
                        when ('0', '0', '11111', '101', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // ST3_asisdlsop_D3_i3d
                        when ('0', '1', _, '010', _, 'x1') => __UNALLOCATED
                        when ('0', '1', _, '011', _, 'x1') => __UNALLOCATED
                        when ('0', '1', _, '100', _, '10') => __UNALLOCATED
                        when ('0', '1', _, '100', '0', '11') => __UNALLOCATED
                        when ('0', '1', _, '100', '1', 'x1') => __UNALLOCATED
                        when ('0', '1', _, '101', _, '10') => __UNALLOCATED
                        when ('0', '1', _, '101', '0', '11') => __UNALLOCATED
                        when ('0', '1', _, '101', '1', 'x1') => __UNALLOCATED
                        when ('0', '1', !'11111', '000', _, _) => __encoding aarch64_memory_vector_single_post_inc // ST2_asisdlsop_BX2_r2b
                        when ('0', '1', !'11111', '001', _, _) => __encoding aarch64_memory_vector_single_post_inc // ST4_asisdlsop_BX4_r4b
                        when ('0', '1', !'11111', '010', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // ST2_asisdlsop_HX2_r2h
                        when ('0', '1', !'11111', '011', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // ST4_asisdlsop_HX4_r4h
                        when ('0', '1', !'11111', '100', _, '00') => __encoding aarch64_memory_vector_single_post_inc // ST2_asisdlsop_SX2_r2s
                        when ('0', '1', !'11111', '100', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // ST2_asisdlsop_DX2_r2d
                        when ('0', '1', !'11111', '101', _, '00') => __encoding aarch64_memory_vector_single_post_inc // ST4_asisdlsop_SX4_r4s
                        when ('0', '1', !'11111', '101', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // ST4_asisdlsop_DX4_r4d
                        when ('0', '1', '11111', '000', _, _) => __encoding aarch64_memory_vector_single_post_inc // ST2_asisdlsop_B2_i2b
                        when ('0', '1', '11111', '001', _, _) => __encoding aarch64_memory_vector_single_post_inc // ST4_asisdlsop_B4_i4b
                        when ('0', '1', '11111', '010', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // ST2_asisdlsop_H2_i2h
                        when ('0', '1', '11111', '011', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // ST4_asisdlsop_H4_i4h
                        when ('0', '1', '11111', '100', _, '00') => __encoding aarch64_memory_vector_single_post_inc // ST2_asisdlsop_S2_i2s
                        when ('0', '1', '11111', '100', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // ST2_asisdlsop_D2_i2d
                        when ('0', '1', '11111', '101', _, '00') => __encoding aarch64_memory_vector_single_post_inc // ST4_asisdlsop_S4_i4s
                        when ('0', '1', '11111', '101', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // ST4_asisdlsop_D4_i4d
                        when ('1', '0', _, '010', _, 'x1') => __UNALLOCATED
                        when ('1', '0', _, '011', _, 'x1') => __UNALLOCATED
                        when ('1', '0', _, '100', _, '1x') => __UNALLOCATED
                        when ('1', '0', _, '100', '1', '01') => __UNALLOCATED
                        when ('1', '0', _, '101', _, '10') => __UNALLOCATED
                        when ('1', '0', _, '101', '0', '11') => __UNALLOCATED
                        when ('1', '0', _, '101', '1', 'x1') => __UNALLOCATED
                        when ('1', '0', _, '110', '1', _) => __UNALLOCATED
                        when ('1', '0', _, '111', '1', _) => __UNALLOCATED
                        when ('1', '0', !'11111', '000', _, _) => __encoding aarch64_memory_vector_single_post_inc // LD1_asisdlsop_BX1_r1b
                        when ('1', '0', !'11111', '001', _, _) => __encoding aarch64_memory_vector_single_post_inc // LD3_asisdlsop_BX3_r3b
                        when ('1', '0', !'11111', '010', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // LD1_asisdlsop_HX1_r1h
                        when ('1', '0', !'11111', '011', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // LD3_asisdlsop_HX3_r3h
                        when ('1', '0', !'11111', '100', _, '00') => __encoding aarch64_memory_vector_single_post_inc // LD1_asisdlsop_SX1_r1s
                        when ('1', '0', !'11111', '100', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // LD1_asisdlsop_DX1_r1d
                        when ('1', '0', !'11111', '101', _, '00') => __encoding aarch64_memory_vector_single_post_inc // LD3_asisdlsop_SX3_r3s
                        when ('1', '0', !'11111', '101', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // LD3_asisdlsop_DX3_r3d
                        when ('1', '0', !'11111', '110', '0', _) => __encoding aarch64_memory_vector_single_post_inc // LD1R_asisdlsop_RX1_r
                        when ('1', '0', !'11111', '111', '0', _) => __encoding aarch64_memory_vector_single_post_inc // LD3R_asisdlsop_RX3_r
                        when ('1', '0', '11111', '000', _, _) => __encoding aarch64_memory_vector_single_post_inc // LD1_asisdlsop_B1_i1b
                        when ('1', '0', '11111', '001', _, _) => __encoding aarch64_memory_vector_single_post_inc // LD3_asisdlsop_B3_i3b
                        when ('1', '0', '11111', '010', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // LD1_asisdlsop_H1_i1h
                        when ('1', '0', '11111', '011', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // LD3_asisdlsop_H3_i3h
                        when ('1', '0', '11111', '100', _, '00') => __encoding aarch64_memory_vector_single_post_inc // LD1_asisdlsop_S1_i1s
                        when ('1', '0', '11111', '100', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // LD1_asisdlsop_D1_i1d
                        when ('1', '0', '11111', '101', _, '00') => __encoding aarch64_memory_vector_single_post_inc // LD3_asisdlsop_S3_i3s
                        when ('1', '0', '11111', '101', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // LD3_asisdlsop_D3_i3d
                        when ('1', '0', '11111', '110', '0', _) => __encoding aarch64_memory_vector_single_post_inc // LD1R_asisdlsop_R1_i
                        when ('1', '0', '11111', '111', '0', _) => __encoding aarch64_memory_vector_single_post_inc // LD3R_asisdlsop_R3_i
                        when ('1', '1', _, '010', _, 'x1') => __UNALLOCATED
                        when ('1', '1', _, '011', _, 'x1') => __UNALLOCATED
                        when ('1', '1', _, '100', _, '10') => __UNALLOCATED
                        when ('1', '1', _, '100', '0', '11') => __UNALLOCATED
                        when ('1', '1', _, '100', '1', 'x1') => __UNALLOCATED
                        when ('1', '1', _, '101', _, '10') => __UNALLOCATED
                        when ('1', '1', _, '101', '0', '11') => __UNALLOCATED
                        when ('1', '1', _, '101', '1', 'x1') => __UNALLOCATED
                        when ('1', '1', _, '110', '1', _) => __UNALLOCATED
                        when ('1', '1', _, '111', '1', _) => __UNALLOCATED
                        when ('1', '1', !'11111', '000', _, _) => __encoding aarch64_memory_vector_single_post_inc // LD2_asisdlsop_BX2_r2b
                        when ('1', '1', !'11111', '001', _, _) => __encoding aarch64_memory_vector_single_post_inc // LD4_asisdlsop_BX4_r4b
                        when ('1', '1', !'11111', '010', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // LD2_asisdlsop_HX2_r2h
                        when ('1', '1', !'11111', '011', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // LD4_asisdlsop_HX4_r4h
                        when ('1', '1', !'11111', '100', _, '00') => __encoding aarch64_memory_vector_single_post_inc // LD2_asisdlsop_SX2_r2s
                        when ('1', '1', !'11111', '100', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // LD2_asisdlsop_DX2_r2d
                        when ('1', '1', !'11111', '101', _, '00') => __encoding aarch64_memory_vector_single_post_inc // LD4_asisdlsop_SX4_r4s
                        when ('1', '1', !'11111', '101', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // LD4_asisdlsop_DX4_r4d
                        when ('1', '1', !'11111', '110', '0', _) => __encoding aarch64_memory_vector_single_post_inc // LD2R_asisdlsop_RX2_r
                        when ('1', '1', !'11111', '111', '0', _) => __encoding aarch64_memory_vector_single_post_inc // LD4R_asisdlsop_RX4_r
                        when ('1', '1', '11111', '000', _, _) => __encoding aarch64_memory_vector_single_post_inc // LD2_asisdlsop_B2_i2b
                        when ('1', '1', '11111', '001', _, _) => __encoding aarch64_memory_vector_single_post_inc // LD4_asisdlsop_B4_i4b
                        when ('1', '1', '11111', '010', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // LD2_asisdlsop_H2_i2h
                        when ('1', '1', '11111', '011', _, 'x0') => __encoding aarch64_memory_vector_single_post_inc // LD4_asisdlsop_H4_i4h
                        when ('1', '1', '11111', '100', _, '00') => __encoding aarch64_memory_vector_single_post_inc // LD2_asisdlsop_S2_i2s
                        when ('1', '1', '11111', '100', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // LD2_asisdlsop_D2_i2d
                        when ('1', '1', '11111', '101', _, '00') => __encoding aarch64_memory_vector_single_post_inc // LD4_asisdlsop_S4_i4s
                        when ('1', '1', '11111', '101', '0', '01') => __encoding aarch64_memory_vector_single_post_inc // LD4_asisdlsop_D4_i4d
                        when ('1', '1', '11111', '110', '0', _) => __encoding aarch64_memory_vector_single_post_inc // LD2R_asisdlsop_R2_i
                        when ('1', '1', '11111', '111', '0', _) => __encoding aarch64_memory_vector_single_post_inc // LD4R_asisdlsop_R4_i
                when ('0x00', _, '1', _, 'x0', _, 'x1xxxx', _, _, _) => __UNPREDICTABLE
                when ('0x00', _, '1', _, 'x0', _, 'xx1xxx', _, _, _) => __UNPREDICTABLE
                when ('0x00', _, '1', _, 'x0', _, 'xxx1xx', _, _, _) => __UNPREDICTABLE
                when ('0x00', _, '1', _, 'x0', _, 'xxxx1x', _, _, _) => __UNPREDICTABLE
                when ('0x00', _, '1', _, 'x0', _, 'xxxxx1', _, _, _) => __UNPREDICTABLE
                when ('1101', _, '0', _, '1x', _, '1xxxxx', _, _, _) => // ldsttags
                    __field opc 22 +: 2
                    __field imm9 12 +: 9
                    __field op2 10 +: 2
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (opc, imm9, op2) of
                        when ('00', _, '00') => __UNALLOCATED
                        when ('00', _, '01') => __encoding aarch64_integer_tags_mcsettagpost // STG_64Spost_ldsttags
                        when ('00', _, '10') => __encoding aarch64_integer_tags_mcsettag // STG_64Soffset_ldsttags
                        when ('00', _, '11') => __encoding aarch64_integer_tags_mcsettagpre // STG_64Spre_ldsttags
                        when ('01', _, '00') => __encoding aarch64_integer_tags_mcgettag // LDG_64Loffset_ldsttags
                        when ('01', _, '01') => __encoding aarch64_integer_tags_mcsettagandzerodatapost // STZG_64Spost_ldsttags
                        when ('01', _, '10') => __encoding aarch64_integer_tags_mcsettagandzerodata // STZG_64Soffset_ldsttags
                        when ('01', _, '11') => __encoding aarch64_integer_tags_mcsettagandzerodatapre // STZG_64Spre_ldsttags
                        when ('10', _, '01') => __encoding aarch64_integer_tags_mcsettagpairpost // ST2G_64Spost_ldsttags
                        when ('10', _, '10') => __encoding aarch64_integer_tags_mcsettagpair // ST2G_64Soffset_ldsttags
                        when ('10', _, '11') => __encoding aarch64_integer_tags_mcsettagpairpre // ST2G_64Spre_ldsttags
                        when ('10', !'000000000', '00') => __UNALLOCATED
                        when ('10', '000000000', '00') => __encoding aarch64_integer_tags_mcsettagarray // STGV_64bulk_ldsttags
                        when ('11', _, '01') => __encoding aarch64_integer_tags_mcsettagpairandzerodatapost // STZ2G_64Spost_ldsttags
                        when ('11', _, '10') => __encoding aarch64_integer_tags_mcsettagpairandzerodata // STZ2G_64Soffset_ldsttags
                        when ('11', _, '11') => __encoding aarch64_integer_tags_mcsettagpairandzerodatapre // STZ2G_64Spre_ldsttags
                        when ('11', !'000000000', '00') => __UNALLOCATED
                        when ('11', '000000000', '00') => __encoding aarch64_integer_tags_mcgettagarray // LDGV_64bulk_ldsttags
                when ('1x00', _, '1', _, _, _, _, _, _, _) => __UNPREDICTABLE
                when ('xx00', _, '0', _, '0x', _, _, _, _, _) => // ldstexcl
                    __field size 30 +: 2
                    __field o2 23 +: 1
                    __field L 22 +: 1
                    __field o1 21 +: 1
                    __field Rs 16 +: 5
                    __field o0 15 +: 1
                    __field Rt2 10 +: 5
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (size, o2, L, o1, o0, Rt2) of
                        when (_, '1', _, '1', _, !'11111') => __UNALLOCATED
                        when ('0x', '0', _, '1', _, !'11111') => __UNALLOCATED
                        when ('00', '0', '0', '0', '0', _) => __encoding aarch64_memory_exclusive_single // STXRB_SR32_ldstexcl
                        when ('00', '0', '0', '0', '1', _) => __encoding aarch64_memory_exclusive_single // STLXRB_SR32_ldstexcl
                        when ('00', '0', '0', '1', '0', '11111') => __encoding aarch64_memory_atomicops_cas_pair // CASP_CP32_ldstexcl
                        when ('00', '0', '0', '1', '1', '11111') => __encoding aarch64_memory_atomicops_cas_pair // CASPL_CP32_ldstexcl
                        when ('00', '0', '1', '0', '0', _) => __encoding aarch64_memory_exclusive_single // LDXRB_LR32_ldstexcl
                        when ('00', '0', '1', '0', '1', _) => __encoding aarch64_memory_exclusive_single // LDAXRB_LR32_ldstexcl
                        when ('00', '0', '1', '1', '0', '11111') => __encoding aarch64_memory_atomicops_cas_pair // CASPA_CP32_ldstexcl
                        when ('00', '0', '1', '1', '1', '11111') => __encoding aarch64_memory_atomicops_cas_pair // CASPAL_CP32_ldstexcl
                        when ('00', '1', '0', '0', '0', _) => __encoding aarch64_memory_ordered // STLLRB_SL32_ldstexcl
                        when ('00', '1', '0', '0', '1', _) => __encoding aarch64_memory_ordered // STLRB_SL32_ldstexcl
                        when ('00', '1', '0', '1', '0', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASB_C32_ldstexcl
                        when ('00', '1', '0', '1', '1', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASLB_C32_ldstexcl
                        when ('00', '1', '1', '0', '0', _) => __encoding aarch64_memory_ordered // LDLARB_LR32_ldstexcl
                        when ('00', '1', '1', '0', '1', _) => __encoding aarch64_memory_ordered // LDARB_LR32_ldstexcl
                        when ('00', '1', '1', '1', '0', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASAB_C32_ldstexcl
                        when ('00', '1', '1', '1', '1', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASALB_C32_ldstexcl
                        when ('01', '0', '0', '0', '0', _) => __encoding aarch64_memory_exclusive_single // STXRH_SR32_ldstexcl
                        when ('01', '0', '0', '0', '1', _) => __encoding aarch64_memory_exclusive_single // STLXRH_SR32_ldstexcl
                        when ('01', '0', '0', '1', '0', '11111') => __encoding aarch64_memory_atomicops_cas_pair // CASP_CP64_ldstexcl
                        when ('01', '0', '0', '1', '1', '11111') => __encoding aarch64_memory_atomicops_cas_pair // CASPL_CP64_ldstexcl
                        when ('01', '0', '1', '0', '0', _) => __encoding aarch64_memory_exclusive_single // LDXRH_LR32_ldstexcl
                        when ('01', '0', '1', '0', '1', _) => __encoding aarch64_memory_exclusive_single // LDAXRH_LR32_ldstexcl
                        when ('01', '0', '1', '1', '0', '11111') => __encoding aarch64_memory_atomicops_cas_pair // CASPA_CP64_ldstexcl
                        when ('01', '0', '1', '1', '1', '11111') => __encoding aarch64_memory_atomicops_cas_pair // CASPAL_CP64_ldstexcl
                        when ('01', '1', '0', '0', '0', _) => __encoding aarch64_memory_ordered // STLLRH_SL32_ldstexcl
                        when ('01', '1', '0', '0', '1', _) => __encoding aarch64_memory_ordered // STLRH_SL32_ldstexcl
                        when ('01', '1', '0', '1', '0', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASH_C32_ldstexcl
                        when ('01', '1', '0', '1', '1', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASLH_C32_ldstexcl
                        when ('01', '1', '1', '0', '0', _) => __encoding aarch64_memory_ordered // LDLARH_LR32_ldstexcl
                        when ('01', '1', '1', '0', '1', _) => __encoding aarch64_memory_ordered // LDARH_LR32_ldstexcl
                        when ('01', '1', '1', '1', '0', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASAH_C32_ldstexcl
                        when ('01', '1', '1', '1', '1', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASALH_C32_ldstexcl
                        when ('10', '0', '0', '0', '0', _) => __encoding aarch64_memory_exclusive_single // STXR_SR32_ldstexcl
                        when ('10', '0', '0', '0', '1', _) => __encoding aarch64_memory_exclusive_single // STLXR_SR32_ldstexcl
                        when ('10', '0', '0', '1', '0', _) => __encoding aarch64_memory_exclusive_pair // STXP_SP32_ldstexcl
                        when ('10', '0', '0', '1', '1', _) => __encoding aarch64_memory_exclusive_pair // STLXP_SP32_ldstexcl
                        when ('10', '0', '1', '0', '0', _) => __encoding aarch64_memory_exclusive_single // LDXR_LR32_ldstexcl
                        when ('10', '0', '1', '0', '1', _) => __encoding aarch64_memory_exclusive_single // LDAXR_LR32_ldstexcl
                        when ('10', '0', '1', '1', '0', _) => __encoding aarch64_memory_exclusive_pair // LDXP_LP32_ldstexcl
                        when ('10', '0', '1', '1', '1', _) => __encoding aarch64_memory_exclusive_pair // LDAXP_LP32_ldstexcl
                        when ('10', '1', '0', '0', '0', _) => __encoding aarch64_memory_ordered // STLLR_SL32_ldstexcl
                        when ('10', '1', '0', '0', '1', _) => __encoding aarch64_memory_ordered // STLR_SL32_ldstexcl
                        when ('10', '1', '0', '1', '0', '11111') => __encoding aarch64_memory_atomicops_cas_single // CAS_C32_ldstexcl
                        when ('10', '1', '0', '1', '1', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASL_C32_ldstexcl
                        when ('10', '1', '1', '0', '0', _) => __encoding aarch64_memory_ordered // LDLAR_LR32_ldstexcl
                        when ('10', '1', '1', '0', '1', _) => __encoding aarch64_memory_ordered // LDAR_LR32_ldstexcl
                        when ('10', '1', '1', '1', '0', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASA_C32_ldstexcl
                        when ('10', '1', '1', '1', '1', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASAL_C32_ldstexcl
                        when ('11', '0', '0', '0', '0', _) => __encoding aarch64_memory_exclusive_single // STXR_SR64_ldstexcl
                        when ('11', '0', '0', '0', '1', _) => __encoding aarch64_memory_exclusive_single // STLXR_SR64_ldstexcl
                        when ('11', '0', '0', '1', '0', _) => __encoding aarch64_memory_exclusive_pair // STXP_SP64_ldstexcl
                        when ('11', '0', '0', '1', '1', _) => __encoding aarch64_memory_exclusive_pair // STLXP_SP64_ldstexcl
                        when ('11', '0', '1', '0', '0', _) => __encoding aarch64_memory_exclusive_single // LDXR_LR64_ldstexcl
                        when ('11', '0', '1', '0', '1', _) => __encoding aarch64_memory_exclusive_single // LDAXR_LR64_ldstexcl
                        when ('11', '0', '1', '1', '0', _) => __encoding aarch64_memory_exclusive_pair // LDXP_LP64_ldstexcl
                        when ('11', '0', '1', '1', '1', _) => __encoding aarch64_memory_exclusive_pair // LDAXP_LP64_ldstexcl
                        when ('11', '1', '0', '0', '0', _) => __encoding aarch64_memory_ordered // STLLR_SL64_ldstexcl
                        when ('11', '1', '0', '0', '1', _) => __encoding aarch64_memory_ordered // STLR_SL64_ldstexcl
                        when ('11', '1', '0', '1', '0', '11111') => __encoding aarch64_memory_atomicops_cas_single // CAS_C64_ldstexcl
                        when ('11', '1', '0', '1', '1', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASL_C64_ldstexcl
                        when ('11', '1', '1', '0', '0', _) => __encoding aarch64_memory_ordered // LDLAR_LR64_ldstexcl
                        when ('11', '1', '1', '0', '1', _) => __encoding aarch64_memory_ordered // LDAR_LR64_ldstexcl
                        when ('11', '1', '1', '1', '0', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASA_C64_ldstexcl
                        when ('11', '1', '1', '1', '1', '11111') => __encoding aarch64_memory_atomicops_cas_single // CASAL_C64_ldstexcl
                when ('xx01', _, '0', _, '1x', _, '0xxxxx', _, '00', _) => // ldapstl_unscaled
                    __field size 30 +: 2
                    __field opc 22 +: 2
                    __field imm9 12 +: 9
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (size, opc) of
                        when ('00', '00') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // STLURB_32_ldapstl_unscaled
                        when ('00', '01') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // LDAPURB_32_ldapstl_unscaled
                        when ('00', '10') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // LDAPURSB_64_ldapstl_unscaled
                        when ('00', '11') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // LDAPURSB_32_ldapstl_unscaled
                        when ('01', '00') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // STLURH_32_ldapstl_unscaled
                        when ('01', '01') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // LDAPURH_32_ldapstl_unscaled
                        when ('01', '10') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // LDAPURSH_64_ldapstl_unscaled
                        when ('01', '11') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // LDAPURSH_32_ldapstl_unscaled
                        when ('10', '00') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // STLUR_32_ldapstl_unscaled
                        when ('10', '01') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // LDAPUR_32_ldapstl_unscaled
                        when ('10', '10') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // LDAPURSW_64_ldapstl_unscaled
                        when ('10', '11') => __UNALLOCATED
                        when ('11', '00') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // STLUR_64_ldapstl_unscaled
                        when ('11', '01') => __encoding aarch64_memory_single_general_immediate_signed_offset_lda_stl // LDAPUR_64_ldapstl_unscaled
                        when ('11', '10') => __UNALLOCATED
                        when ('11', '11') => __UNALLOCATED
                when ('xx01', _, _, _, '0x', _, _, _, _, _) => // loadlit
                    __field opc 30 +: 2
                    __field V 26 +: 1
                    __field imm19 5 +: 19
                    __field Rt 0 +: 5
                    case (opc, V) of
                        when ('00', '0') => __encoding aarch64_memory_literal_general // LDR_32_loadlit
                        when ('00', '1') => __encoding aarch64_memory_literal_simdfp // LDR_S_loadlit
                        when ('01', '0') => __encoding aarch64_memory_literal_general // LDR_64_loadlit
                        when ('01', '1') => __encoding aarch64_memory_literal_simdfp // LDR_D_loadlit
                        when ('10', '0') => __encoding aarch64_memory_literal_general // LDRSW_64_loadlit
                        when ('10', '1') => __encoding aarch64_memory_literal_simdfp // LDR_Q_loadlit
                        when ('11', '0') => __encoding aarch64_memory_literal_general // PRFM_P_loadlit
                        when ('11', '1') => __UNALLOCATED
                when ('xx10', _, _, _, '00', _, _, _, _, _) => // ldstnapair_offs
                    __field opc 30 +: 2
                    __field V 26 +: 1
                    __field L 22 +: 1
                    __field imm7 15 +: 7
                    __field Rt2 10 +: 5
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (opc, V, L) of
                        when ('00', '0', '0') => __encoding aarch64_memory_pair_general_no_alloc // STNP_32_ldstnapair_offs
                        when ('00', '0', '1') => __encoding aarch64_memory_pair_general_no_alloc // LDNP_32_ldstnapair_offs
                        when ('00', '1', '0') => __encoding aarch64_memory_pair_simdfp_no_alloc // STNP_S_ldstnapair_offs
                        when ('00', '1', '1') => __encoding aarch64_memory_pair_simdfp_no_alloc // LDNP_S_ldstnapair_offs
                        when ('01', '0', _) => __UNALLOCATED
                        when ('01', '1', '0') => __encoding aarch64_memory_pair_simdfp_no_alloc // STNP_D_ldstnapair_offs
                        when ('01', '1', '1') => __encoding aarch64_memory_pair_simdfp_no_alloc // LDNP_D_ldstnapair_offs
                        when ('10', '0', '0') => __encoding aarch64_memory_pair_general_no_alloc // STNP_64_ldstnapair_offs
                        when ('10', '0', '1') => __encoding aarch64_memory_pair_general_no_alloc // LDNP_64_ldstnapair_offs
                        when ('10', '1', '0') => __encoding aarch64_memory_pair_simdfp_no_alloc // STNP_Q_ldstnapair_offs
                        when ('10', '1', '1') => __encoding aarch64_memory_pair_simdfp_no_alloc // LDNP_Q_ldstnapair_offs
                        when ('11', _, _) => __UNALLOCATED
                when ('xx10', _, _, _, '01', _, _, _, _, _) => // ldstpair_post
                    __field opc 30 +: 2
                    __field V 26 +: 1
                    __field L 22 +: 1
                    __field imm7 15 +: 7
                    __field Rt2 10 +: 5
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (opc, V, L) of
                        when ('00', '0', '0') => __encoding aarch64_memory_pair_general_post_idx // STP_32_ldstpair_post
                        when ('00', '0', '1') => __encoding aarch64_memory_pair_general_post_idx // LDP_32_ldstpair_post
                        when ('00', '1', '0') => __encoding aarch64_memory_pair_simdfp_post_idx // STP_S_ldstpair_post
                        when ('00', '1', '1') => __encoding aarch64_memory_pair_simdfp_post_idx // LDP_S_ldstpair_post
                        when ('01', '0', '0') => __encoding aarch64_integer_tags_mcsettaganddatapairpost // STGP_64_ldstpair_post
                        when ('01', '0', '1') => __encoding aarch64_memory_pair_general_post_idx // LDPSW_64_ldstpair_post
                        when ('01', '1', '0') => __encoding aarch64_memory_pair_simdfp_post_idx // STP_D_ldstpair_post
                        when ('01', '1', '1') => __encoding aarch64_memory_pair_simdfp_post_idx // LDP_D_ldstpair_post
                        when ('10', '0', '0') => __encoding aarch64_memory_pair_general_post_idx // STP_64_ldstpair_post
                        when ('10', '0', '1') => __encoding aarch64_memory_pair_general_post_idx // LDP_64_ldstpair_post
                        when ('10', '1', '0') => __encoding aarch64_memory_pair_simdfp_post_idx // STP_Q_ldstpair_post
                        when ('10', '1', '1') => __encoding aarch64_memory_pair_simdfp_post_idx // LDP_Q_ldstpair_post
                        when ('11', _, _) => __UNALLOCATED
                when ('xx10', _, _, _, '10', _, _, _, _, _) => // ldstpair_off
                    __field opc 30 +: 2
                    __field V 26 +: 1
                    __field L 22 +: 1
                    __field imm7 15 +: 7
                    __field Rt2 10 +: 5
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (opc, V, L) of
                        when ('00', '0', '0') => __encoding aarch64_memory_pair_general_offset // STP_32_ldstpair_off
                        when ('00', '0', '1') => __encoding aarch64_memory_pair_general_offset // LDP_32_ldstpair_off
                        when ('00', '1', '0') => __encoding aarch64_memory_pair_simdfp_offset // STP_S_ldstpair_off
                        when ('00', '1', '1') => __encoding aarch64_memory_pair_simdfp_offset // LDP_S_ldstpair_off
                        when ('01', '0', '0') => __encoding aarch64_integer_tags_mcsettaganddatapair // STGP_64_ldstpair_off
                        when ('01', '0', '1') => __encoding aarch64_memory_pair_general_offset // LDPSW_64_ldstpair_off
                        when ('01', '1', '0') => __encoding aarch64_memory_pair_simdfp_offset // STP_D_ldstpair_off
                        when ('01', '1', '1') => __encoding aarch64_memory_pair_simdfp_offset // LDP_D_ldstpair_off
                        when ('10', '0', '0') => __encoding aarch64_memory_pair_general_offset // STP_64_ldstpair_off
                        when ('10', '0', '1') => __encoding aarch64_memory_pair_general_offset // LDP_64_ldstpair_off
                        when ('10', '1', '0') => __encoding aarch64_memory_pair_simdfp_offset // STP_Q_ldstpair_off
                        when ('10', '1', '1') => __encoding aarch64_memory_pair_simdfp_offset // LDP_Q_ldstpair_off
                        when ('11', _, _) => __UNALLOCATED
                when ('xx10', _, _, _, '11', _, _, _, _, _) => // ldstpair_pre
                    __field opc 30 +: 2
                    __field V 26 +: 1
                    __field L 22 +: 1
                    __field imm7 15 +: 7
                    __field Rt2 10 +: 5
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (opc, V, L) of
                        when ('00', '0', '0') => __encoding aarch64_memory_pair_general_pre_idx // STP_32_ldstpair_pre
                        when ('00', '0', '1') => __encoding aarch64_memory_pair_general_pre_idx // LDP_32_ldstpair_pre
                        when ('00', '1', '0') => __encoding aarch64_memory_pair_simdfp_pre_idx // STP_S_ldstpair_pre
                        when ('00', '1', '1') => __encoding aarch64_memory_pair_simdfp_pre_idx // LDP_S_ldstpair_pre
                        when ('01', '0', '0') => __encoding aarch64_integer_tags_mcsettaganddatapairpre // STGP_64_ldstpair_pre
                        when ('01', '0', '1') => __encoding aarch64_memory_pair_general_pre_idx // LDPSW_64_ldstpair_pre
                        when ('01', '1', '0') => __encoding aarch64_memory_pair_simdfp_pre_idx // STP_D_ldstpair_pre
                        when ('01', '1', '1') => __encoding aarch64_memory_pair_simdfp_pre_idx // LDP_D_ldstpair_pre
                        when ('10', '0', '0') => __encoding aarch64_memory_pair_general_pre_idx // STP_64_ldstpair_pre
                        when ('10', '0', '1') => __encoding aarch64_memory_pair_general_pre_idx // LDP_64_ldstpair_pre
                        when ('10', '1', '0') => __encoding aarch64_memory_pair_simdfp_pre_idx // STP_Q_ldstpair_pre
                        when ('10', '1', '1') => __encoding aarch64_memory_pair_simdfp_pre_idx // LDP_Q_ldstpair_pre
                        when ('11', _, _) => __UNALLOCATED
                when ('xx11', _, _, _, '0x', _, '0xxxxx', _, '00', _) => // ldst_unscaled
                    __field size 30 +: 2
                    __field V 26 +: 1
                    __field opc 22 +: 2
                    __field imm9 12 +: 9
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (size, V, opc) of
                        when ('x1', '1', '1x') => __UNALLOCATED
                        when ('00', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // STURB_32_ldst_unscaled
                        when ('00', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // LDURB_32_ldst_unscaled
                        when ('00', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // LDURSB_64_ldst_unscaled
                        when ('00', '0', '11') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // LDURSB_32_ldst_unscaled
                        when ('00', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_signed_offset_normal // STUR_B_ldst_unscaled
                        when ('00', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_signed_offset_normal // LDUR_B_ldst_unscaled
                        when ('00', '1', '10') => __encoding aarch64_memory_single_simdfp_immediate_signed_offset_normal // STUR_Q_ldst_unscaled
                        when ('00', '1', '11') => __encoding aarch64_memory_single_simdfp_immediate_signed_offset_normal // LDUR_Q_ldst_unscaled
                        when ('01', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // STURH_32_ldst_unscaled
                        when ('01', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // LDURH_32_ldst_unscaled
                        when ('01', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // LDURSH_64_ldst_unscaled
                        when ('01', '0', '11') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // LDURSH_32_ldst_unscaled
                        when ('01', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_signed_offset_normal // STUR_H_ldst_unscaled
                        when ('01', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_signed_offset_normal // LDUR_H_ldst_unscaled
                        when ('1x', '0', '11') => __UNALLOCATED
                        when ('1x', '1', '1x') => __UNALLOCATED
                        when ('10', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // STUR_32_ldst_unscaled
                        when ('10', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // LDUR_32_ldst_unscaled
                        when ('10', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // LDURSW_64_ldst_unscaled
                        when ('10', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_signed_offset_normal // STUR_S_ldst_unscaled
                        when ('10', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_signed_offset_normal // LDUR_S_ldst_unscaled
                        when ('11', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // STUR_64_ldst_unscaled
                        when ('11', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // LDUR_64_ldst_unscaled
                        when ('11', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_offset_normal // PRFUM_P_ldst_unscaled
                        when ('11', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_signed_offset_normal // STUR_D_ldst_unscaled
                        when ('11', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_signed_offset_normal // LDUR_D_ldst_unscaled
                when ('xx11', _, _, _, '0x', _, '0xxxxx', _, '01', _) => // ldst_immpost
                    __field size 30 +: 2
                    __field V 26 +: 1
                    __field opc 22 +: 2
                    __field imm9 12 +: 9
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (size, V, opc) of
                        when ('x1', '1', '1x') => __UNALLOCATED
                        when ('00', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // STRB_32_ldst_immpost
                        when ('00', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // LDRB_32_ldst_immpost
                        when ('00', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // LDRSB_64_ldst_immpost
                        when ('00', '0', '11') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // LDRSB_32_ldst_immpost
                        when ('00', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_signed_post_idx // STR_B_ldst_immpost
                        when ('00', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_signed_post_idx // LDR_B_ldst_immpost
                        when ('00', '1', '10') => __encoding aarch64_memory_single_simdfp_immediate_signed_post_idx // STR_Q_ldst_immpost
                        when ('00', '1', '11') => __encoding aarch64_memory_single_simdfp_immediate_signed_post_idx // LDR_Q_ldst_immpost
                        when ('01', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // STRH_32_ldst_immpost
                        when ('01', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // LDRH_32_ldst_immpost
                        when ('01', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // LDRSH_64_ldst_immpost
                        when ('01', '0', '11') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // LDRSH_32_ldst_immpost
                        when ('01', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_signed_post_idx // STR_H_ldst_immpost
                        when ('01', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_signed_post_idx // LDR_H_ldst_immpost
                        when ('1x', '0', '11') => __UNALLOCATED
                        when ('1x', '1', '1x') => __UNALLOCATED
                        when ('10', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // STR_32_ldst_immpost
                        when ('10', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // LDR_32_ldst_immpost
                        when ('10', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // LDRSW_64_ldst_immpost
                        when ('10', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_signed_post_idx // STR_S_ldst_immpost
                        when ('10', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_signed_post_idx // LDR_S_ldst_immpost
                        when ('11', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // STR_64_ldst_immpost
                        when ('11', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_post_idx // LDR_64_ldst_immpost
                        when ('11', '0', '10') => __UNALLOCATED
                        when ('11', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_signed_post_idx // STR_D_ldst_immpost
                        when ('11', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_signed_post_idx // LDR_D_ldst_immpost
                when ('xx11', _, _, _, '0x', _, '0xxxxx', _, '10', _) => // ldst_unpriv
                    __field size 30 +: 2
                    __field V 26 +: 1
                    __field opc 22 +: 2
                    __field imm9 12 +: 9
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (size, V, opc) of
                        when (_, '1', _) => __UNALLOCATED
                        when ('00', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // STTRB_32_ldst_unpriv
                        when ('00', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // LDTRB_32_ldst_unpriv
                        when ('00', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // LDTRSB_64_ldst_unpriv
                        when ('00', '0', '11') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // LDTRSB_32_ldst_unpriv
                        when ('01', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // STTRH_32_ldst_unpriv
                        when ('01', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // LDTRH_32_ldst_unpriv
                        when ('01', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // LDTRSH_64_ldst_unpriv
                        when ('01', '0', '11') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // LDTRSH_32_ldst_unpriv
                        when ('1x', '0', '11') => __UNALLOCATED
                        when ('10', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // STTR_32_ldst_unpriv
                        when ('10', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // LDTR_32_ldst_unpriv
                        when ('10', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // LDTRSW_64_ldst_unpriv
                        when ('11', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // STTR_64_ldst_unpriv
                        when ('11', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_offset_unpriv // LDTR_64_ldst_unpriv
                        when ('11', '0', '10') => __UNALLOCATED
                when ('xx11', _, _, _, '0x', _, '0xxxxx', _, '11', _) => // ldst_immpre
                    __field size 30 +: 2
                    __field V 26 +: 1
                    __field opc 22 +: 2
                    __field imm9 12 +: 9
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (size, V, opc) of
                        when ('x1', '1', '1x') => __UNALLOCATED
                        when ('00', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // STRB_32_ldst_immpre
                        when ('00', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // LDRB_32_ldst_immpre
                        when ('00', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // LDRSB_64_ldst_immpre
                        when ('00', '0', '11') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // LDRSB_32_ldst_immpre
                        when ('00', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_signed_pre_idx // STR_B_ldst_immpre
                        when ('00', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_signed_pre_idx // LDR_B_ldst_immpre
                        when ('00', '1', '10') => __encoding aarch64_memory_single_simdfp_immediate_signed_pre_idx // STR_Q_ldst_immpre
                        when ('00', '1', '11') => __encoding aarch64_memory_single_simdfp_immediate_signed_pre_idx // LDR_Q_ldst_immpre
                        when ('01', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // STRH_32_ldst_immpre
                        when ('01', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // LDRH_32_ldst_immpre
                        when ('01', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // LDRSH_64_ldst_immpre
                        when ('01', '0', '11') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // LDRSH_32_ldst_immpre
                        when ('01', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_signed_pre_idx // STR_H_ldst_immpre
                        when ('01', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_signed_pre_idx // LDR_H_ldst_immpre
                        when ('1x', '0', '11') => __UNALLOCATED
                        when ('1x', '1', '1x') => __UNALLOCATED
                        when ('10', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // STR_32_ldst_immpre
                        when ('10', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // LDR_32_ldst_immpre
                        when ('10', '0', '10') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // LDRSW_64_ldst_immpre
                        when ('10', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_signed_pre_idx // STR_S_ldst_immpre
                        when ('10', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_signed_pre_idx // LDR_S_ldst_immpre
                        when ('11', '0', '00') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // STR_64_ldst_immpre
                        when ('11', '0', '01') => __encoding aarch64_memory_single_general_immediate_signed_pre_idx // LDR_64_ldst_immpre
                        when ('11', '0', '10') => __UNALLOCATED
                        when ('11', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_signed_pre_idx // STR_D_ldst_immpre
                        when ('11', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_signed_pre_idx // LDR_D_ldst_immpre
                when ('xx11', _, _, _, '0x', _, '1xxxxx', _, '00', _) => // memop
                    __field size 30 +: 2
                    __field V 26 +: 1
                    __field A 23 +: 1
                    __field R 22 +: 1
                    __field Rs 16 +: 5
                    __field o3 15 +: 1
                    __field opc 12 +: 3
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (size, V, A, R, o3, opc) of
                        when (_, '0', _, _, '1', '001') => __UNALLOCATED
                        when (_, '0', _, _, '1', '01x') => __UNALLOCATED
                        when (_, '0', _, _, '1', '101') => __UNALLOCATED
                        when (_, '0', _, _, '1', '11x') => __UNALLOCATED
                        when (_, '0', '0', _, '1', '100') => __UNALLOCATED
                        when (_, '0', '1', '1', '1', '100') => __UNALLOCATED
                        when (_, '1', _, _, _, _) => __UNALLOCATED
                        when ('00', '0', '0', '0', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDB_32_memop
                        when ('00', '0', '0', '0', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRB_32_memop
                        when ('00', '0', '0', '0', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORB_32_memop
                        when ('00', '0', '0', '0', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETB_32_memop
                        when ('00', '0', '0', '0', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXB_32_memop
                        when ('00', '0', '0', '0', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINB_32_memop
                        when ('00', '0', '0', '0', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXB_32_memop
                        when ('00', '0', '0', '0', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINB_32_memop
                        when ('00', '0', '0', '0', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPB_32_memop
                        when ('00', '0', '0', '1', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDLB_32_memop
                        when ('00', '0', '0', '1', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRLB_32_memop
                        when ('00', '0', '0', '1', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORLB_32_memop
                        when ('00', '0', '0', '1', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETLB_32_memop
                        when ('00', '0', '0', '1', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXLB_32_memop
                        when ('00', '0', '0', '1', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINLB_32_memop
                        when ('00', '0', '0', '1', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXLB_32_memop
                        when ('00', '0', '0', '1', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINLB_32_memop
                        when ('00', '0', '0', '1', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPLB_32_memop
                        when ('00', '0', '1', '0', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDAB_32_memop
                        when ('00', '0', '1', '0', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRAB_32_memop
                        when ('00', '0', '1', '0', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORAB_32_memop
                        when ('00', '0', '1', '0', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETAB_32_memop
                        when ('00', '0', '1', '0', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXAB_32_memop
                        when ('00', '0', '1', '0', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINAB_32_memop
                        when ('00', '0', '1', '0', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXAB_32_memop
                        when ('00', '0', '1', '0', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINAB_32_memop
                        when ('00', '0', '1', '0', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPAB_32_memop
                        when ('00', '0', '1', '0', '1', '100') => __encoding aarch64_memory_ordered_rcpc // LDAPRB_32L_memop
                        when ('00', '0', '1', '1', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDALB_32_memop
                        when ('00', '0', '1', '1', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRALB_32_memop
                        when ('00', '0', '1', '1', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORALB_32_memop
                        when ('00', '0', '1', '1', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETALB_32_memop
                        when ('00', '0', '1', '1', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXALB_32_memop
                        when ('00', '0', '1', '1', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINALB_32_memop
                        when ('00', '0', '1', '1', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXALB_32_memop
                        when ('00', '0', '1', '1', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINALB_32_memop
                        when ('00', '0', '1', '1', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPALB_32_memop
                        when ('01', '0', '0', '0', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDH_32_memop
                        when ('01', '0', '0', '0', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRH_32_memop
                        when ('01', '0', '0', '0', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORH_32_memop
                        when ('01', '0', '0', '0', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETH_32_memop
                        when ('01', '0', '0', '0', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXH_32_memop
                        when ('01', '0', '0', '0', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINH_32_memop
                        when ('01', '0', '0', '0', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXH_32_memop
                        when ('01', '0', '0', '0', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINH_32_memop
                        when ('01', '0', '0', '0', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPH_32_memop
                        when ('01', '0', '0', '1', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDLH_32_memop
                        when ('01', '0', '0', '1', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRLH_32_memop
                        when ('01', '0', '0', '1', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORLH_32_memop
                        when ('01', '0', '0', '1', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETLH_32_memop
                        when ('01', '0', '0', '1', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXLH_32_memop
                        when ('01', '0', '0', '1', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINLH_32_memop
                        when ('01', '0', '0', '1', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXLH_32_memop
                        when ('01', '0', '0', '1', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINLH_32_memop
                        when ('01', '0', '0', '1', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPLH_32_memop
                        when ('01', '0', '1', '0', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDAH_32_memop
                        when ('01', '0', '1', '0', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRAH_32_memop
                        when ('01', '0', '1', '0', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORAH_32_memop
                        when ('01', '0', '1', '0', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETAH_32_memop
                        when ('01', '0', '1', '0', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXAH_32_memop
                        when ('01', '0', '1', '0', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINAH_32_memop
                        when ('01', '0', '1', '0', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXAH_32_memop
                        when ('01', '0', '1', '0', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINAH_32_memop
                        when ('01', '0', '1', '0', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPAH_32_memop
                        when ('01', '0', '1', '0', '1', '100') => __encoding aarch64_memory_ordered_rcpc // LDAPRH_32L_memop
                        when ('01', '0', '1', '1', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDALH_32_memop
                        when ('01', '0', '1', '1', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRALH_32_memop
                        when ('01', '0', '1', '1', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORALH_32_memop
                        when ('01', '0', '1', '1', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETALH_32_memop
                        when ('01', '0', '1', '1', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXALH_32_memop
                        when ('01', '0', '1', '1', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINALH_32_memop
                        when ('01', '0', '1', '1', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXALH_32_memop
                        when ('01', '0', '1', '1', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINALH_32_memop
                        when ('01', '0', '1', '1', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPALH_32_memop
                        when ('10', '0', '0', '0', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADD_32_memop
                        when ('10', '0', '0', '0', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLR_32_memop
                        when ('10', '0', '0', '0', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEOR_32_memop
                        when ('10', '0', '0', '0', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSET_32_memop
                        when ('10', '0', '0', '0', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAX_32_memop
                        when ('10', '0', '0', '0', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMIN_32_memop
                        when ('10', '0', '0', '0', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAX_32_memop
                        when ('10', '0', '0', '0', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMIN_32_memop
                        when ('10', '0', '0', '0', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWP_32_memop
                        when ('10', '0', '0', '1', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDL_32_memop
                        when ('10', '0', '0', '1', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRL_32_memop
                        when ('10', '0', '0', '1', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORL_32_memop
                        when ('10', '0', '0', '1', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETL_32_memop
                        when ('10', '0', '0', '1', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXL_32_memop
                        when ('10', '0', '0', '1', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINL_32_memop
                        when ('10', '0', '0', '1', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXL_32_memop
                        when ('10', '0', '0', '1', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINL_32_memop
                        when ('10', '0', '0', '1', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPL_32_memop
                        when ('10', '0', '1', '0', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDA_32_memop
                        when ('10', '0', '1', '0', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRA_32_memop
                        when ('10', '0', '1', '0', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORA_32_memop
                        when ('10', '0', '1', '0', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETA_32_memop
                        when ('10', '0', '1', '0', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXA_32_memop
                        when ('10', '0', '1', '0', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINA_32_memop
                        when ('10', '0', '1', '0', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXA_32_memop
                        when ('10', '0', '1', '0', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINA_32_memop
                        when ('10', '0', '1', '0', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPA_32_memop
                        when ('10', '0', '1', '0', '1', '100') => __encoding aarch64_memory_ordered_rcpc // LDAPR_32L_memop
                        when ('10', '0', '1', '1', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDAL_32_memop
                        when ('10', '0', '1', '1', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRAL_32_memop
                        when ('10', '0', '1', '1', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORAL_32_memop
                        when ('10', '0', '1', '1', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETAL_32_memop
                        when ('10', '0', '1', '1', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXAL_32_memop
                        when ('10', '0', '1', '1', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINAL_32_memop
                        when ('10', '0', '1', '1', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXAL_32_memop
                        when ('10', '0', '1', '1', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINAL_32_memop
                        when ('10', '0', '1', '1', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPAL_32_memop
                        when ('11', '0', '0', '0', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADD_64_memop
                        when ('11', '0', '0', '0', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLR_64_memop
                        when ('11', '0', '0', '0', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEOR_64_memop
                        when ('11', '0', '0', '0', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSET_64_memop
                        when ('11', '0', '0', '0', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAX_64_memop
                        when ('11', '0', '0', '0', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMIN_64_memop
                        when ('11', '0', '0', '0', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAX_64_memop
                        when ('11', '0', '0', '0', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMIN_64_memop
                        when ('11', '0', '0', '0', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWP_64_memop
                        when ('11', '0', '0', '1', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDL_64_memop
                        when ('11', '0', '0', '1', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRL_64_memop
                        when ('11', '0', '0', '1', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORL_64_memop
                        when ('11', '0', '0', '1', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETL_64_memop
                        when ('11', '0', '0', '1', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXL_64_memop
                        when ('11', '0', '0', '1', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINL_64_memop
                        when ('11', '0', '0', '1', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXL_64_memop
                        when ('11', '0', '0', '1', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINL_64_memop
                        when ('11', '0', '0', '1', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPL_64_memop
                        when ('11', '0', '1', '0', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDA_64_memop
                        when ('11', '0', '1', '0', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRA_64_memop
                        when ('11', '0', '1', '0', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORA_64_memop
                        when ('11', '0', '1', '0', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETA_64_memop
                        when ('11', '0', '1', '0', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXA_64_memop
                        when ('11', '0', '1', '0', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINA_64_memop
                        when ('11', '0', '1', '0', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXA_64_memop
                        when ('11', '0', '1', '0', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINA_64_memop
                        when ('11', '0', '1', '0', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPA_64_memop
                        when ('11', '0', '1', '0', '1', '100') => __encoding aarch64_memory_ordered_rcpc // LDAPR_64L_memop
                        when ('11', '0', '1', '1', '0', '000') => __encoding aarch64_memory_atomicops_ld // LDADDAL_64_memop
                        when ('11', '0', '1', '1', '0', '001') => __encoding aarch64_memory_atomicops_ld // LDCLRAL_64_memop
                        when ('11', '0', '1', '1', '0', '010') => __encoding aarch64_memory_atomicops_ld // LDEORAL_64_memop
                        when ('11', '0', '1', '1', '0', '011') => __encoding aarch64_memory_atomicops_ld // LDSETAL_64_memop
                        when ('11', '0', '1', '1', '0', '100') => __encoding aarch64_memory_atomicops_ld // LDSMAXAL_64_memop
                        when ('11', '0', '1', '1', '0', '101') => __encoding aarch64_memory_atomicops_ld // LDSMINAL_64_memop
                        when ('11', '0', '1', '1', '0', '110') => __encoding aarch64_memory_atomicops_ld // LDUMAXAL_64_memop
                        when ('11', '0', '1', '1', '0', '111') => __encoding aarch64_memory_atomicops_ld // LDUMINAL_64_memop
                        when ('11', '0', '1', '1', '1', '000') => __encoding aarch64_memory_atomicops_swp // SWPAL_64_memop
                when ('xx11', _, _, _, '0x', _, '1xxxxx', _, '10', _) => // ldst_regoff
                    __field size 30 +: 2
                    __field V 26 +: 1
                    __field opc 22 +: 2
                    __field Rm 16 +: 5
                    __field option 13 +: 3
                    __field S 12 +: 1
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (size, V, opc, option) of
                        when (_, _, _, 'x0x') => __UNALLOCATED
                        when ('x1', '1', '1x', _) => __UNALLOCATED
                        when ('00', '0', '00', !'011') => __encoding aarch64_memory_single_general_register // STRB_32B_ldst_regoff
                        when ('00', '0', '00', '011') => __encoding aarch64_memory_single_general_register // STRB_32BL_ldst_regoff
                        when ('00', '0', '01', !'011') => __encoding aarch64_memory_single_general_register // LDRB_32B_ldst_regoff
                        when ('00', '0', '01', '011') => __encoding aarch64_memory_single_general_register // LDRB_32BL_ldst_regoff
                        when ('00', '0', '10', !'011') => __encoding aarch64_memory_single_general_register // LDRSB_64B_ldst_regoff
                        when ('00', '0', '10', '011') => __encoding aarch64_memory_single_general_register // LDRSB_64BL_ldst_regoff
                        when ('00', '0', '11', !'011') => __encoding aarch64_memory_single_general_register // LDRSB_32B_ldst_regoff
                        when ('00', '0', '11', '011') => __encoding aarch64_memory_single_general_register // LDRSB_32BL_ldst_regoff
                        when ('00', '1', '00', !'011') => __encoding aarch64_memory_single_simdfp_register // STR_B_ldst_regoff
                        when ('00', '1', '00', '011') => __encoding aarch64_memory_single_simdfp_register // STR_BL_ldst_regoff
                        when ('00', '1', '01', !'011') => __encoding aarch64_memory_single_simdfp_register // LDR_B_ldst_regoff
                        when ('00', '1', '01', '011') => __encoding aarch64_memory_single_simdfp_register // LDR_BL_ldst_regoff
                        when ('00', '1', '10', _) => __encoding aarch64_memory_single_simdfp_register // STR_Q_ldst_regoff
                        when ('00', '1', '11', _) => __encoding aarch64_memory_single_simdfp_register // LDR_Q_ldst_regoff
                        when ('01', '0', '00', _) => __encoding aarch64_memory_single_general_register // STRH_32_ldst_regoff
                        when ('01', '0', '01', _) => __encoding aarch64_memory_single_general_register // LDRH_32_ldst_regoff
                        when ('01', '0', '10', _) => __encoding aarch64_memory_single_general_register // LDRSH_64_ldst_regoff
                        when ('01', '0', '11', _) => __encoding aarch64_memory_single_general_register // LDRSH_32_ldst_regoff
                        when ('01', '1', '00', _) => __encoding aarch64_memory_single_simdfp_register // STR_H_ldst_regoff
                        when ('01', '1', '01', _) => __encoding aarch64_memory_single_simdfp_register // LDR_H_ldst_regoff
                        when ('1x', '0', '11', _) => __UNALLOCATED
                        when ('1x', '1', '1x', _) => __UNALLOCATED
                        when ('10', '0', '00', _) => __encoding aarch64_memory_single_general_register // STR_32_ldst_regoff
                        when ('10', '0', '01', _) => __encoding aarch64_memory_single_general_register // LDR_32_ldst_regoff
                        when ('10', '0', '10', _) => __encoding aarch64_memory_single_general_register // LDRSW_64_ldst_regoff
                        when ('10', '1', '00', _) => __encoding aarch64_memory_single_simdfp_register // STR_S_ldst_regoff
                        when ('10', '1', '01', _) => __encoding aarch64_memory_single_simdfp_register // LDR_S_ldst_regoff
                        when ('11', '0', '00', _) => __encoding aarch64_memory_single_general_register // STR_64_ldst_regoff
                        when ('11', '0', '01', _) => __encoding aarch64_memory_single_general_register // LDR_64_ldst_regoff
                        when ('11', '0', '10', _) => __encoding aarch64_memory_single_general_register // PRFM_P_ldst_regoff
                        when ('11', '1', '00', _) => __encoding aarch64_memory_single_simdfp_register // STR_D_ldst_regoff
                        when ('11', '1', '01', _) => __encoding aarch64_memory_single_simdfp_register // LDR_D_ldst_regoff
                when ('xx11', _, _, _, '0x', _, '1xxxxx', _, 'x1', _) => // ldst_pac
                    __field size 30 +: 2
                    __field V 26 +: 1
                    __field M 23 +: 1
                    __field S 22 +: 1
                    __field imm9 12 +: 9
                    __field W 11 +: 1
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (size, V, M, W) of
                        when (!'11', _, _, _) => __UNALLOCATED
                        when ('11', '0', '0', '0') => __encoding aarch64_memory_single_general_immediate_signed_pac // LDRAA_64_ldst_pac
                        when ('11', '0', '0', '1') => __encoding aarch64_memory_single_general_immediate_signed_pac // LDRAA_64W_ldst_pac
                        when ('11', '0', '1', '0') => __encoding aarch64_memory_single_general_immediate_signed_pac // LDRAB_64_ldst_pac
                        when ('11', '0', '1', '1') => __encoding aarch64_memory_single_general_immediate_signed_pac // LDRAB_64W_ldst_pac
                        when ('11', '1', _, _) => __UNALLOCATED
                when ('xx11', _, _, _, '1x', _, _, _, _, _) => // ldst_pos
                    __field size 30 +: 2
                    __field V 26 +: 1
                    __field opc 22 +: 2
                    __field imm12 10 +: 12
                    __field Rn 5 +: 5
                    __field Rt 0 +: 5
                    case (size, V, opc) of
                        when ('x1', '1', '1x') => __UNALLOCATED
                        when ('00', '0', '00') => __encoding aarch64_memory_single_general_immediate_unsigned // STRB_32_ldst_pos
                        when ('00', '0', '01') => __encoding aarch64_memory_single_general_immediate_unsigned // LDRB_32_ldst_pos
                        when ('00', '0', '10') => __encoding aarch64_memory_single_general_immediate_unsigned // LDRSB_64_ldst_pos
                        when ('00', '0', '11') => __encoding aarch64_memory_single_general_immediate_unsigned // LDRSB_32_ldst_pos
                        when ('00', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_unsigned // STR_B_ldst_pos
                        when ('00', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_unsigned // LDR_B_ldst_pos
                        when ('00', '1', '10') => __encoding aarch64_memory_single_simdfp_immediate_unsigned // STR_Q_ldst_pos
                        when ('00', '1', '11') => __encoding aarch64_memory_single_simdfp_immediate_unsigned // LDR_Q_ldst_pos
                        when ('01', '0', '00') => __encoding aarch64_memory_single_general_immediate_unsigned // STRH_32_ldst_pos
                        when ('01', '0', '01') => __encoding aarch64_memory_single_general_immediate_unsigned // LDRH_32_ldst_pos
                        when ('01', '0', '10') => __encoding aarch64_memory_single_general_immediate_unsigned // LDRSH_64_ldst_pos
                        when ('01', '0', '11') => __encoding aarch64_memory_single_general_immediate_unsigned // LDRSH_32_ldst_pos
                        when ('01', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_unsigned // STR_H_ldst_pos
                        when ('01', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_unsigned // LDR_H_ldst_pos
                        when ('1x', '0', '11') => __UNALLOCATED
                        when ('1x', '1', '1x') => __UNALLOCATED
                        when ('10', '0', '00') => __encoding aarch64_memory_single_general_immediate_unsigned // STR_32_ldst_pos
                        when ('10', '0', '01') => __encoding aarch64_memory_single_general_immediate_unsigned // LDR_32_ldst_pos
                        when ('10', '0', '10') => __encoding aarch64_memory_single_general_immediate_unsigned // LDRSW_64_ldst_pos
                        when ('10', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_unsigned // STR_S_ldst_pos
                        when ('10', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_unsigned // LDR_S_ldst_pos
                        when ('11', '0', '00') => __encoding aarch64_memory_single_general_immediate_unsigned // STR_64_ldst_pos
                        when ('11', '0', '01') => __encoding aarch64_memory_single_general_immediate_unsigned // LDR_64_ldst_pos
                        when ('11', '0', '10') => __encoding aarch64_memory_single_general_immediate_unsigned // PRFM_P_ldst_pos
                        when ('11', '1', '00') => __encoding aarch64_memory_single_simdfp_immediate_unsigned // STR_D_ldst_pos
                        when ('11', '1', '01') => __encoding aarch64_memory_single_simdfp_immediate_unsigned // LDR_D_ldst_pos
        when (_, 'x101', _) =>
            // dpreg
            case (31 +: 1, 30 +: 1, 29 +: 1, 28 +: 1, 25 +: 3, 21 +: 4, 16 +: 5, 10 +: 6, 0 +: 10) of
                when (_, '0', _, '1', _, '0110', _, _, _) => // dp_2src
                    __field sf 31 +: 1
                    __field S 29 +: 1
                    __field Rm 16 +: 5
                    __field opcode 10 +: 6
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, S, opcode) of
                        when (_, _, '000001') => __UNALLOCATED
                        when (_, _, '011xxx') => __UNALLOCATED
                        when (_, _, '1xxxxx') => __UNALLOCATED
                        when (_, '0', '000101') => __UNALLOCATED
                        when (_, '0', '0011xx') => __UNALLOCATED
                        when (_, '1', '00001x') => __UNALLOCATED
                        when (_, '1', '0001xx') => __UNALLOCATED
                        when (_, '1', '001xxx') => __UNALLOCATED
                        when (_, '1', '01xxxx') => __UNALLOCATED
                        when ('0', _, '000000') => __UNALLOCATED
                        when ('0', '0', '000010') => __encoding aarch64_integer_arithmetic_div // UDIV_32_dp_2src
                        when ('0', '0', '000011') => __encoding aarch64_integer_arithmetic_div // SDIV_32_dp_2src
                        when ('0', '0', '000100') => __UNALLOCATED
                        when ('0', '0', '00011x') => __UNALLOCATED
                        when ('0', '0', '001000') => __encoding aarch64_integer_shift_variable // LSLV_32_dp_2src
                        when ('0', '0', '001001') => __encoding aarch64_integer_shift_variable // LSRV_32_dp_2src
                        when ('0', '0', '001010') => __encoding aarch64_integer_shift_variable // ASRV_32_dp_2src
                        when ('0', '0', '001011') => __encoding aarch64_integer_shift_variable // RORV_32_dp_2src
                        when ('0', '0', '010x11') => __UNALLOCATED
                        when ('0', '0', '010000') => __encoding aarch64_integer_crc // CRC32B_32C_dp_2src
                        when ('0', '0', '010001') => __encoding aarch64_integer_crc // CRC32H_32C_dp_2src
                        when ('0', '0', '010010') => __encoding aarch64_integer_crc // CRC32W_32C_dp_2src
                        when ('0', '0', '010100') => __encoding aarch64_integer_crc // CRC32CB_32C_dp_2src
                        when ('0', '0', '010101') => __encoding aarch64_integer_crc // CRC32CH_32C_dp_2src
                        when ('0', '0', '010110') => __encoding aarch64_integer_crc // CRC32CW_32C_dp_2src
                        when ('1', '0', '000000') => __encoding aarch64_integer_arithmetic_pointer_mcsubtracttaggedaddress // SUBP_64S_dp_2src
                        when ('1', '0', '000010') => __encoding aarch64_integer_arithmetic_div // UDIV_64_dp_2src
                        when ('1', '0', '000011') => __encoding aarch64_integer_arithmetic_div // SDIV_64_dp_2src
                        when ('1', '0', '000100') => __encoding aarch64_integer_tags_mcinsertrandomtag // IRG_64I_dp_2src
                        when ('1', '0', '000101') => __encoding aarch64_integer_tags_mcinserttagmask // GMI_64G_dp_2src
                        when ('1', '0', '001000') => __encoding aarch64_integer_shift_variable // LSLV_64_dp_2src
                        when ('1', '0', '001001') => __encoding aarch64_integer_shift_variable // LSRV_64_dp_2src
                        when ('1', '0', '001010') => __encoding aarch64_integer_shift_variable // ASRV_64_dp_2src
                        when ('1', '0', '001011') => __encoding aarch64_integer_shift_variable // RORV_64_dp_2src
                        when ('1', '0', '001100') => __encoding aarch64_integer_pac_pacga_dp_2src // PACGA_64P_dp_2src
                        when ('1', '0', '010xx0') => __UNALLOCATED
                        when ('1', '0', '010x0x') => __UNALLOCATED
                        when ('1', '0', '010011') => __encoding aarch64_integer_crc // CRC32X_64C_dp_2src
                        when ('1', '0', '010111') => __encoding aarch64_integer_crc // CRC32CX_64C_dp_2src
                        when ('1', '1', '000000') => __encoding aarch64_integer_arithmetic_pointer_mcsubtracttaggedaddresssetflags // SUBPS_64S_dp_2src
                when (_, '1', _, '1', _, '0110', _, _, _) => // dp_1src
                    __field sf 31 +: 1
                    __field S 29 +: 1
                    __field opcode2 16 +: 5
                    __field opcode 10 +: 6
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, S, opcode2, opcode, Rn) of
                        when (_, _, _, 'xx1xxx', _) => __UNALLOCATED
                        when (_, _, _, 'x1xxxx', _) => __UNALLOCATED
                        when (_, _, _, '1xxxxx', _) => __UNALLOCATED
                        when (_, _, 'xxxx1', _, _) => __UNALLOCATED
                        when (_, _, 'xxx1x', _, _) => __UNALLOCATED
                        when (_, _, 'xx1xx', _, _) => __UNALLOCATED
                        when (_, _, 'x1xxx', _, _) => __UNALLOCATED
                        when (_, _, '1xxxx', _, _) => __UNALLOCATED
                        when (_, '0', '00000', '00011x', _) => __UNALLOCATED
                        when (_, '1', _, _, _) => __UNALLOCATED
                        when ('0', '0', '00000', '000000', _) => __encoding aarch64_integer_arithmetic_rbit // RBIT_32_dp_1src
                        when ('0', '0', '00000', '000001', _) => __encoding aarch64_integer_arithmetic_rev // REV16_32_dp_1src
                        when ('0', '0', '00000', '000010', _) => __encoding aarch64_integer_arithmetic_rev // REV_32_dp_1src
                        when ('0', '0', '00000', '000011', _) => __UNALLOCATED
                        when ('0', '0', '00000', '000100', _) => __encoding aarch64_integer_arithmetic_cnt // CLZ_32_dp_1src
                        when ('0', '0', '00000', '000101', _) => __encoding aarch64_integer_arithmetic_cnt // CLS_32_dp_1src
                        when ('1', '0', '00000', '000000', _) => __encoding aarch64_integer_arithmetic_rbit // RBIT_64_dp_1src
                        when ('1', '0', '00000', '000001', _) => __encoding aarch64_integer_arithmetic_rev // REV16_64_dp_1src
                        when ('1', '0', '00000', '000010', _) => __encoding aarch64_integer_arithmetic_rev // REV32_64_dp_1src
                        when ('1', '0', '00000', '000011', _) => __encoding aarch64_integer_arithmetic_rev // REV_64_dp_1src
                        when ('1', '0', '00000', '000100', _) => __encoding aarch64_integer_arithmetic_cnt // CLZ_64_dp_1src
                        when ('1', '0', '00000', '000101', _) => __encoding aarch64_integer_arithmetic_cnt // CLS_64_dp_1src
                        when ('1', '0', '00001', '000000', _) => __encoding aarch64_integer_pac_pacia_dp_1src // PACIA_64P_dp_1src
                        when ('1', '0', '00001', '000001', _) => __encoding aarch64_integer_pac_pacib_dp_1src // PACIB_64P_dp_1src
                        when ('1', '0', '00001', '000010', _) => __encoding aarch64_integer_pac_pacda_dp_1src // PACDA_64P_dp_1src
                        when ('1', '0', '00001', '000011', _) => __encoding aarch64_integer_pac_pacdb_dp_1src // PACDB_64P_dp_1src
                        when ('1', '0', '00001', '000100', _) => __encoding aarch64_integer_pac_autia_dp_1src // AUTIA_64P_dp_1src
                        when ('1', '0', '00001', '000101', _) => __encoding aarch64_integer_pac_autib_dp_1src // AUTIB_64P_dp_1src
                        when ('1', '0', '00001', '000110', _) => __encoding aarch64_integer_pac_autda_dp_1src // AUTDA_64P_dp_1src
                        when ('1', '0', '00001', '000111', _) => __encoding aarch64_integer_pac_autdb_dp_1src // AUTDB_64P_dp_1src
                        when ('1', '0', '00001', '001000', '11111') => __encoding aarch64_integer_pac_pacia_dp_1src // PACIZA_64Z_dp_1src
                        when ('1', '0', '00001', '001001', '11111') => __encoding aarch64_integer_pac_pacib_dp_1src // PACIZB_64Z_dp_1src
                        when ('1', '0', '00001', '001010', '11111') => __encoding aarch64_integer_pac_pacda_dp_1src // PACDZA_64Z_dp_1src
                        when ('1', '0', '00001', '001011', '11111') => __encoding aarch64_integer_pac_pacdb_dp_1src // PACDZB_64Z_dp_1src
                        when ('1', '0', '00001', '001100', '11111') => __encoding aarch64_integer_pac_autia_dp_1src // AUTIZA_64Z_dp_1src
                        when ('1', '0', '00001', '001101', '11111') => __encoding aarch64_integer_pac_autib_dp_1src // AUTIZB_64Z_dp_1src
                        when ('1', '0', '00001', '001110', '11111') => __encoding aarch64_integer_pac_autda_dp_1src // AUTDZA_64Z_dp_1src
                        when ('1', '0', '00001', '001111', '11111') => __encoding aarch64_integer_pac_autdb_dp_1src // AUTDZB_64Z_dp_1src
                        when ('1', '0', '00001', '010000', '11111') => __encoding aarch64_integer_pac_strip_dp_1src // XPACI_64Z_dp_1src
                        when ('1', '0', '00001', '010001', '11111') => __encoding aarch64_integer_pac_strip_dp_1src // XPACD_64Z_dp_1src
                when (_, _, _, '0', _, '0xxx', _, _, _) => // log_shift
                    __field sf 31 +: 1
                    __field opc 29 +: 2
                    __field shift 22 +: 2
                    __field N 21 +: 1
                    __field Rm 16 +: 5
                    __field imm6 10 +: 6
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, opc, N, imm6) of
                        when ('0', _, _, '1xxxxx') => __UNALLOCATED
                        when ('0', '00', '0', _) => __encoding aarch64_integer_logical_shiftedreg // AND_32_log_shift
                        when ('0', '00', '1', _) => __encoding aarch64_integer_logical_shiftedreg // BIC_32_log_shift
                        when ('0', '01', '0', _) => __encoding aarch64_integer_logical_shiftedreg // ORR_32_log_shift
                        when ('0', '01', '1', _) => __encoding aarch64_integer_logical_shiftedreg // ORN_32_log_shift
                        when ('0', '10', '0', _) => __encoding aarch64_integer_logical_shiftedreg // EOR_32_log_shift
                        when ('0', '10', '1', _) => __encoding aarch64_integer_logical_shiftedreg // EON_32_log_shift
                        when ('0', '11', '0', _) => __encoding aarch64_integer_logical_shiftedreg // ANDS_32_log_shift
                        when ('0', '11', '1', _) => __encoding aarch64_integer_logical_shiftedreg // BICS_32_log_shift
                        when ('1', '00', '0', _) => __encoding aarch64_integer_logical_shiftedreg // AND_64_log_shift
                        when ('1', '00', '1', _) => __encoding aarch64_integer_logical_shiftedreg // BIC_64_log_shift
                        when ('1', '01', '0', _) => __encoding aarch64_integer_logical_shiftedreg // ORR_64_log_shift
                        when ('1', '01', '1', _) => __encoding aarch64_integer_logical_shiftedreg // ORN_64_log_shift
                        when ('1', '10', '0', _) => __encoding aarch64_integer_logical_shiftedreg // EOR_64_log_shift
                        when ('1', '10', '1', _) => __encoding aarch64_integer_logical_shiftedreg // EON_64_log_shift
                        when ('1', '11', '0', _) => __encoding aarch64_integer_logical_shiftedreg // ANDS_64_log_shift
                        when ('1', '11', '1', _) => __encoding aarch64_integer_logical_shiftedreg // BICS_64_log_shift
                when (_, _, _, '0', _, '1xx0', _, _, _) => // addsub_shift
                    __field sf 31 +: 1
                    __field op 30 +: 1
                    __field S 29 +: 1
                    __field shift 22 +: 2
                    __field Rm 16 +: 5
                    __field imm6 10 +: 6
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, op, S, shift, imm6) of
                        when (_, _, _, '11', _) => __UNALLOCATED
                        when ('0', _, _, _, '1xxxxx') => __UNALLOCATED
                        when ('0', '0', '0', _, _) => __encoding aarch64_integer_arithmetic_add_sub_shiftedreg // ADD_32_addsub_shift
                        when ('0', '0', '1', _, _) => __encoding aarch64_integer_arithmetic_add_sub_shiftedreg // ADDS_32_addsub_shift
                        when ('0', '1', '0', _, _) => __encoding aarch64_integer_arithmetic_add_sub_shiftedreg // SUB_32_addsub_shift
                        when ('0', '1', '1', _, _) => __encoding aarch64_integer_arithmetic_add_sub_shiftedreg // SUBS_32_addsub_shift
                        when ('1', '0', '0', _, _) => __encoding aarch64_integer_arithmetic_add_sub_shiftedreg // ADD_64_addsub_shift
                        when ('1', '0', '1', _, _) => __encoding aarch64_integer_arithmetic_add_sub_shiftedreg // ADDS_64_addsub_shift
                        when ('1', '1', '0', _, _) => __encoding aarch64_integer_arithmetic_add_sub_shiftedreg // SUB_64_addsub_shift
                        when ('1', '1', '1', _, _) => __encoding aarch64_integer_arithmetic_add_sub_shiftedreg // SUBS_64_addsub_shift
                when (_, _, _, '0', _, '1xx1', _, _, _) => // addsub_ext
                    __field sf 31 +: 1
                    __field op 30 +: 1
                    __field S 29 +: 1
                    __field opt 22 +: 2
                    __field Rm 16 +: 5
                    __field option 13 +: 3
                    __field imm3 10 +: 3
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, op, S, opt, imm3) of
                        when (_, _, _, _, '1x1') => __UNALLOCATED
                        when (_, _, _, _, '11x') => __UNALLOCATED
                        when (_, _, _, 'x1', _) => __UNALLOCATED
                        when (_, _, _, '1x', _) => __UNALLOCATED
                        when ('0', '0', '0', '00', _) => __encoding aarch64_integer_arithmetic_add_sub_extendedreg // ADD_32_addsub_ext
                        when ('0', '0', '1', '00', _) => __encoding aarch64_integer_arithmetic_add_sub_extendedreg // ADDS_32S_addsub_ext
                        when ('0', '1', '0', '00', _) => __encoding aarch64_integer_arithmetic_add_sub_extendedreg // SUB_32_addsub_ext
                        when ('0', '1', '1', '00', _) => __encoding aarch64_integer_arithmetic_add_sub_extendedreg // SUBS_32S_addsub_ext
                        when ('1', '0', '0', '00', _) => __encoding aarch64_integer_arithmetic_add_sub_extendedreg // ADD_64_addsub_ext
                        when ('1', '0', '1', '00', _) => __encoding aarch64_integer_arithmetic_add_sub_extendedreg // ADDS_64S_addsub_ext
                        when ('1', '1', '0', '00', _) => __encoding aarch64_integer_arithmetic_add_sub_extendedreg // SUB_64_addsub_ext
                        when ('1', '1', '1', '00', _) => __encoding aarch64_integer_arithmetic_add_sub_extendedreg // SUBS_64S_addsub_ext
                when (_, _, _, '1', _, '0000', _, '000000', _) => // addsub_carry
                    __field sf 31 +: 1
                    __field op 30 +: 1
                    __field S 29 +: 1
                    __field Rm 16 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, op, S) of
                        when ('0', '0', '0') => __encoding aarch64_integer_arithmetic_add_sub_carry // ADC_32_addsub_carry
                        when ('0', '0', '1') => __encoding aarch64_integer_arithmetic_add_sub_carry // ADCS_32_addsub_carry
                        when ('0', '1', '0') => __encoding aarch64_integer_arithmetic_add_sub_carry // SBC_32_addsub_carry
                        when ('0', '1', '1') => __encoding aarch64_integer_arithmetic_add_sub_carry // SBCS_32_addsub_carry
                        when ('1', '0', '0') => __encoding aarch64_integer_arithmetic_add_sub_carry // ADC_64_addsub_carry
                        when ('1', '0', '1') => __encoding aarch64_integer_arithmetic_add_sub_carry // ADCS_64_addsub_carry
                        when ('1', '1', '0') => __encoding aarch64_integer_arithmetic_add_sub_carry // SBC_64_addsub_carry
                        when ('1', '1', '1') => __encoding aarch64_integer_arithmetic_add_sub_carry // SBCS_64_addsub_carry
                when (_, _, _, '1', _, '0000', _, 'x00001', _) => // rmif
                    __field sf 31 +: 1
                    __field op 30 +: 1
                    __field S 29 +: 1
                    __field imm6 15 +: 6
                    __field Rn 5 +: 5
                    __field o2 4 +: 1
                    __field mask 0 +: 4
                    case (sf, op, S, o2) of
                        when ('0', _, _, _) => __UNALLOCATED
                        when ('1', '0', '0', _) => __UNALLOCATED
                        when ('1', '0', '1', '0') => __encoding aarch64_integer_flags_rmif // RMIF_only_rmif
                        when ('1', '0', '1', '1') => __UNALLOCATED
                        when ('1', '1', _, _) => __UNALLOCATED
                when (_, _, _, '1', _, '0000', _, 'xx0010', _) => // setf
                    __field sf 31 +: 1
                    __field op 30 +: 1
                    __field S 29 +: 1
                    __field opcode2 15 +: 6
                    __field sz 14 +: 1
                    __field Rn 5 +: 5
                    __field o3 4 +: 1
                    __field mask 0 +: 4
                    case (sf, op, S, opcode2, sz, o3, mask) of
                        when ('0', '0', '0', _, _, _, _) => __UNALLOCATED
                        when ('0', '0', '1', !'000000', _, _, _) => __UNALLOCATED
                        when ('0', '0', '1', '000000', _, '0', !'1101') => __UNALLOCATED
                        when ('0', '0', '1', '000000', _, '1', _) => __UNALLOCATED
                        when ('0', '0', '1', '000000', '0', '0', '1101') => __encoding aarch64_integer_flags_setf // SETF8_only_setf
                        when ('0', '0', '1', '000000', '1', '0', '1101') => __encoding aarch64_integer_flags_setf // SETF16_only_setf
                        when ('0', '1', _, _, _, _, _) => __UNALLOCATED
                        when ('1', _, _, _, _, _, _) => __UNALLOCATED
                when (_, _, _, '1', _, '0010', _, 'xxxx0x', _) => // condcmp_reg
                    __field sf 31 +: 1
                    __field op 30 +: 1
                    __field S 29 +: 1
                    __field Rm 16 +: 5
                    __field cond 12 +: 4
                    __field o2 10 +: 1
                    __field Rn 5 +: 5
                    __field o3 4 +: 1
                    __field nzcv 0 +: 4
                    case (sf, op, S, o2, o3) of
                        when (_, _, _, _, '1') => __UNALLOCATED
                        when (_, _, _, '1', _) => __UNALLOCATED
                        when (_, _, '0', _, _) => __UNALLOCATED
                        when ('0', '0', '1', '0', '0') => __encoding aarch64_integer_conditional_compare_register // CCMN_32_condcmp_reg
                        when ('0', '1', '1', '0', '0') => __encoding aarch64_integer_conditional_compare_register // CCMP_32_condcmp_reg
                        when ('1', '0', '1', '0', '0') => __encoding aarch64_integer_conditional_compare_register // CCMN_64_condcmp_reg
                        when ('1', '1', '1', '0', '0') => __encoding aarch64_integer_conditional_compare_register // CCMP_64_condcmp_reg
                when (_, _, _, '1', _, '0010', _, 'xxxx1x', _) => // condcmp_imm
                    __field sf 31 +: 1
                    __field op 30 +: 1
                    __field S 29 +: 1
                    __field imm5 16 +: 5
                    __field cond 12 +: 4
                    __field o2 10 +: 1
                    __field Rn 5 +: 5
                    __field o3 4 +: 1
                    __field nzcv 0 +: 4
                    case (sf, op, S, o2, o3) of
                        when (_, _, _, _, '1') => __UNALLOCATED
                        when (_, _, _, '1', _) => __UNALLOCATED
                        when (_, _, '0', _, _) => __UNALLOCATED
                        when ('0', '0', '1', '0', '0') => __encoding aarch64_integer_conditional_compare_immediate // CCMN_32_condcmp_imm
                        when ('0', '1', '1', '0', '0') => __encoding aarch64_integer_conditional_compare_immediate // CCMP_32_condcmp_imm
                        when ('1', '0', '1', '0', '0') => __encoding aarch64_integer_conditional_compare_immediate // CCMN_64_condcmp_imm
                        when ('1', '1', '1', '0', '0') => __encoding aarch64_integer_conditional_compare_immediate // CCMP_64_condcmp_imm
                when (_, _, _, '1', _, '0100', _, _, _) => // condsel
                    __field sf 31 +: 1
                    __field op 30 +: 1
                    __field S 29 +: 1
                    __field Rm 16 +: 5
                    __field cond 12 +: 4
                    __field op2 10 +: 2
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, op, S, op2) of
                        when (_, _, _, '1x') => __UNALLOCATED
                        when (_, _, '1', _) => __UNALLOCATED
                        when ('0', '0', '0', '00') => __encoding aarch64_integer_conditional_select // CSEL_32_condsel
                        when ('0', '0', '0', '01') => __encoding aarch64_integer_conditional_select // CSINC_32_condsel
                        when ('0', '1', '0', '00') => __encoding aarch64_integer_conditional_select // CSINV_32_condsel
                        when ('0', '1', '0', '01') => __encoding aarch64_integer_conditional_select // CSNEG_32_condsel
                        when ('1', '0', '0', '00') => __encoding aarch64_integer_conditional_select // CSEL_64_condsel
                        when ('1', '0', '0', '01') => __encoding aarch64_integer_conditional_select // CSINC_64_condsel
                        when ('1', '1', '0', '00') => __encoding aarch64_integer_conditional_select // CSINV_64_condsel
                        when ('1', '1', '0', '01') => __encoding aarch64_integer_conditional_select // CSNEG_64_condsel
                when (_, _, _, '1', _, '1xxx', _, _, _) => // dp_3src
                    __field sf 31 +: 1
                    __field op54 29 +: 2
                    __field op31 21 +: 3
                    __field Rm 16 +: 5
                    __field o0 15 +: 1
                    __field Ra 10 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, op54, op31, o0) of
                        when (_, '00', '010', '1') => __UNALLOCATED
                        when (_, '00', '011', _) => __UNALLOCATED
                        when (_, '00', '100', _) => __UNALLOCATED
                        when (_, '00', '110', '1') => __UNALLOCATED
                        when (_, '00', '111', _) => __UNALLOCATED
                        when (_, '01', _, _) => __UNALLOCATED
                        when (_, '1x', _, _) => __UNALLOCATED
                        when ('0', '00', '000', '0') => __encoding aarch64_integer_arithmetic_mul_uniform_add_sub // MADD_32A_dp_3src
                        when ('0', '00', '000', '1') => __encoding aarch64_integer_arithmetic_mul_uniform_add_sub // MSUB_32A_dp_3src
                        when ('0', '00', '001', '0') => __UNALLOCATED
                        when ('0', '00', '001', '1') => __UNALLOCATED
                        when ('0', '00', '010', '0') => __UNALLOCATED
                        when ('0', '00', '101', '0') => __UNALLOCATED
                        when ('0', '00', '101', '1') => __UNALLOCATED
                        when ('0', '00', '110', '0') => __UNALLOCATED
                        when ('1', '00', '000', '0') => __encoding aarch64_integer_arithmetic_mul_uniform_add_sub // MADD_64A_dp_3src
                        when ('1', '00', '000', '1') => __encoding aarch64_integer_arithmetic_mul_uniform_add_sub // MSUB_64A_dp_3src
                        when ('1', '00', '001', '0') => __encoding aarch64_integer_arithmetic_mul_widening_32_64 // SMADDL_64WA_dp_3src
                        when ('1', '00', '001', '1') => __encoding aarch64_integer_arithmetic_mul_widening_32_64 // SMSUBL_64WA_dp_3src
                        when ('1', '00', '010', '0') => __encoding aarch64_integer_arithmetic_mul_widening_64_128hi // SMULH_64_dp_3src
                        when ('1', '00', '101', '0') => __encoding aarch64_integer_arithmetic_mul_widening_32_64 // UMADDL_64WA_dp_3src
                        when ('1', '00', '101', '1') => __encoding aarch64_integer_arithmetic_mul_widening_32_64 // UMSUBL_64WA_dp_3src
                        when ('1', '00', '110', '0') => __encoding aarch64_integer_arithmetic_mul_widening_64_128hi // UMULH_64_dp_3src
        when (_, 'x111', _) =>
            // simd-dp
            case (28 +: 4, 25 +: 3, 23 +: 2, 19 +: 4, 10 +: 9, 0 +: 10) of
                when ('0000', _, '0x', 'x101', '00xxxxx10', _) => __UNPREDICTABLE
                when ('0010', _, '0x', 'x101', '00xxxxx10', _) => __UNPREDICTABLE
                when ('0100', _, '0x', 'x101', '00xxxxx10', _) => // cryptoaes
                    __field size 22 +: 2
                    __field opcode 12 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (size, opcode) of
                        when (_, 'x1xxx') => __UNALLOCATED
                        when (_, '000xx') => __UNALLOCATED
                        when (_, '1xxxx') => __UNALLOCATED
                        when ('x1', _) => __UNALLOCATED
                        when ('00', '00100') => __encoding aarch64_vector_crypto_aes_round // AESE_B_cryptoaes
                        when ('00', '00101') => __encoding aarch64_vector_crypto_aes_round // AESD_B_cryptoaes
                        when ('00', '00110') => __encoding aarch64_vector_crypto_aes_mix // AESMC_B_cryptoaes
                        when ('00', '00111') => __encoding aarch64_vector_crypto_aes_mix // AESIMC_B_cryptoaes
                        when ('1x', _) => __UNALLOCATED
                when ('0101', _, '0x', 'x0xx', 'xxx0xxx00', _) => // cryptosha3
                    __field size 22 +: 2
                    __field Rm 16 +: 5
                    __field opcode 12 +: 3
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (size, opcode) of
                        when (_, '111') => __UNALLOCATED
                        when ('x1', _) => __UNALLOCATED
                        when ('00', '000') => __encoding aarch64_vector_crypto_sha3op_sha1_hash_choose // SHA1C_QSV_cryptosha3
                        when ('00', '001') => __encoding aarch64_vector_crypto_sha3op_sha1_hash_parity // SHA1P_QSV_cryptosha3
                        when ('00', '010') => __encoding aarch64_vector_crypto_sha3op_sha1_hash_majority // SHA1M_QSV_cryptosha3
                        when ('00', '011') => __encoding aarch64_vector_crypto_sha3op_sha1_sched0 // SHA1SU0_VVV_cryptosha3
                        when ('00', '100') => __encoding aarch64_vector_crypto_sha3op_sha256_hash // SHA256H_QQV_cryptosha3
                        when ('00', '101') => __encoding aarch64_vector_crypto_sha3op_sha256_hash // SHA256H2_QQV_cryptosha3
                        when ('00', '110') => __encoding aarch64_vector_crypto_sha3op_sha256_sched1 // SHA256SU1_VVV_cryptosha3
                        when ('1x', _) => __UNALLOCATED
                when ('0101', _, '0x', 'x0xx', 'xxx0xxx10', _) => __UNPREDICTABLE
                when ('0101', _, '0x', 'x101', '00xxxxx10', _) => // cryptosha2
                    __field size 22 +: 2
                    __field opcode 12 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (size, opcode) of
                        when (_, 'xx1xx') => __UNALLOCATED
                        when (_, 'x1xxx') => __UNALLOCATED
                        when (_, '1xxxx') => __UNALLOCATED
                        when ('x1', _) => __UNALLOCATED
                        when ('00', '00000') => __encoding aarch64_vector_crypto_sha2op_sha1_hash // SHA1H_SS_cryptosha2
                        when ('00', '00001') => __encoding aarch64_vector_crypto_sha2op_sha1_sched1 // SHA1SU1_VV_cryptosha2
                        when ('00', '00010') => __encoding aarch64_vector_crypto_sha2op_sha256_sched0 // SHA256SU0_VV_cryptosha2
                        when ('00', '00011') => __UNALLOCATED
                        when ('1x', _) => __UNALLOCATED
                when ('0110', _, '0x', 'x101', '00xxxxx10', _) => __UNPREDICTABLE
                when ('0111', _, '0x', 'x0xx', 'xxx0xxxx0', _) => __UNPREDICTABLE
                when ('0111', _, '0x', 'x101', '00xxxxx10', _) => __UNPREDICTABLE
                when ('01x1', _, '00', '00xx', 'xxx0xxxx1', _) => // asisdone
                    __field op 29 +: 1
                    __field imm5 16 +: 5
                    __field imm4 11 +: 4
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (op, imm5, imm4) of
                        when ('0', _, 'xxx1') => __UNALLOCATED
                        when ('0', _, 'xx1x') => __UNALLOCATED
                        when ('0', _, 'x1xx') => __UNALLOCATED
                        when ('0', _, '0000') => __encoding aarch64_vector_transfer_vector_cpy_dup_sisd // DUP_asisdone_only
                        when ('0', _, '1xxx') => __UNALLOCATED
                        when ('0', 'x0000', '0000') => __UNALLOCATED
                        when ('1', _, _) => __UNALLOCATED
                when ('01x1', _, '01', '00xx', 'xxx0xxxx1', _) => __UNPREDICTABLE
                when ('01x1', _, '0x', '0111', '00xxxxx10', _) => __UNPREDICTABLE
                when ('01x1', _, '0x', '10xx', 'xxx00xxx1', _) => // asisdsamefp16
                    __field U 29 +: 1
                    __field a 23 +: 1
                    __field Rm 16 +: 5
                    __field opcode 11 +: 3
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, a, opcode) of
                        when (_, _, '110') => __UNALLOCATED
                        when (_, '1', '011') => __UNALLOCATED
                        when ('0', '0', '011') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp16_extended_sisd // FMULX_asisdsamefp16_only
                        when ('0', '0', '100') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp16_sisd // FCMEQ_asisdsamefp16_only
                        when ('0', '0', '101') => __UNALLOCATED
                        when ('0', '0', '111') => __encoding aarch64_vector_arithmetic_binary_uniform_recps_fp16_sisd // FRECPS_asisdsamefp16_only
                        when ('0', '1', '100') => __UNALLOCATED
                        when ('0', '1', '101') => __UNALLOCATED
                        when ('0', '1', '111') => __encoding aarch64_vector_arithmetic_binary_uniform_rsqrts_fp16_sisd // FRSQRTS_asisdsamefp16_only
                        when ('1', '0', '011') => __UNALLOCATED
                        when ('1', '0', '100') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp16_sisd // FCMGE_asisdsamefp16_only
                        when ('1', '0', '101') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp16_sisd // FACGE_asisdsamefp16_only
                        when ('1', '0', '111') => __UNALLOCATED
                        when ('1', '1', '010') => __encoding aarch64_vector_arithmetic_binary_uniform_sub_fp16_sisd // FABD_asisdsamefp16_only
                        when ('1', '1', '100') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp16_sisd // FCMGT_asisdsamefp16_only
                        when ('1', '1', '101') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp16_sisd // FACGT_asisdsamefp16_only
                        when ('1', '1', '111') => __UNALLOCATED
                when ('01x1', _, '0x', '10xx', 'xxx01xxx1', _) => __UNPREDICTABLE
                when ('01x1', _, '0x', '1111', '00xxxxx10', _) => // asisdmiscfp16
                    __field U 29 +: 1
                    __field a 23 +: 1
                    __field opcode 12 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, a, opcode) of
                        when (_, _, '00xxx') => __UNALLOCATED
                        when (_, _, '010xx') => __UNALLOCATED
                        when (_, _, '10xxx') => __UNALLOCATED
                        when (_, _, '1100x') => __UNALLOCATED
                        when (_, _, '11110') => __UNALLOCATED
                        when (_, '0', '011xx') => __UNALLOCATED
                        when (_, '0', '11111') => __UNALLOCATED
                        when (_, '1', '01111') => __UNALLOCATED
                        when (_, '1', '11100') => __UNALLOCATED
                        when ('0', '0', '11010') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_sisd // FCVTNS_asisdmiscfp16_R
                        when ('0', '0', '11011') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_sisd // FCVTMS_asisdmiscfp16_R
                        when ('0', '0', '11100') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_tieaway_sisd // FCVTAS_asisdmiscfp16_R
                        when ('0', '0', '11101') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_int_sisd // SCVTF_asisdmiscfp16_R
                        when ('0', '1', '01100') => __encoding aarch64_vector_arithmetic_unary_cmp_fp16_bulk_sisd // FCMGT_asisdmiscfp16_FZ
                        when ('0', '1', '01101') => __encoding aarch64_vector_arithmetic_unary_cmp_fp16_bulk_sisd // FCMEQ_asisdmiscfp16_FZ
                        when ('0', '1', '01110') => __encoding aarch64_vector_arithmetic_unary_cmp_fp16_lessthan_sisd // FCMLT_asisdmiscfp16_FZ
                        when ('0', '1', '11010') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_sisd // FCVTPS_asisdmiscfp16_R
                        when ('0', '1', '11011') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_sisd // FCVTZS_asisdmiscfp16_R
                        when ('0', '1', '11101') => __encoding aarch64_vector_arithmetic_unary_special_recip_fp16_sisd // FRECPE_asisdmiscfp16_R
                        when ('0', '1', '11111') => __encoding aarch64_vector_arithmetic_unary_special_frecpx_fp16 // FRECPX_asisdmiscfp16_R
                        when ('1', '0', '11010') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_sisd // FCVTNU_asisdmiscfp16_R
                        when ('1', '0', '11011') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_sisd // FCVTMU_asisdmiscfp16_R
                        when ('1', '0', '11100') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_tieaway_sisd // FCVTAU_asisdmiscfp16_R
                        when ('1', '0', '11101') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_int_sisd // UCVTF_asisdmiscfp16_R
                        when ('1', '1', '01100') => __encoding aarch64_vector_arithmetic_unary_cmp_fp16_bulk_sisd // FCMGE_asisdmiscfp16_FZ
                        when ('1', '1', '01101') => __encoding aarch64_vector_arithmetic_unary_cmp_fp16_bulk_sisd // FCMLE_asisdmiscfp16_FZ
                        when ('1', '1', '01110') => __UNALLOCATED
                        when ('1', '1', '11010') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_sisd // FCVTPU_asisdmiscfp16_R
                        when ('1', '1', '11011') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_sisd // FCVTZU_asisdmiscfp16_R
                        when ('1', '1', '11101') => __encoding aarch64_vector_arithmetic_unary_special_sqrt_est_fp16_sisd // FRSQRTE_asisdmiscfp16_R
                        when ('1', '1', '11111') => __UNALLOCATED
                when ('01x1', _, '0x', 'x0xx', 'xxx1xxxx0', _) => __UNPREDICTABLE
                when ('01x1', _, '0x', 'x0xx', 'xxx1xxxx1', _) => // asisdsame2
                    __field U 29 +: 1
                    __field size 22 +: 2
                    __field Rm 16 +: 5
                    __field opcode 11 +: 4
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, opcode) of
                        when (_, '001x') => __UNALLOCATED
                        when (_, '01xx') => __UNALLOCATED
                        when (_, '1xxx') => __UNALLOCATED
                        when ('0', '0000') => __UNALLOCATED
                        when ('0', '0001') => __UNALLOCATED
                        when ('1', '0000') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_doubling_accum_sisd // SQRDMLAH_asisdsame2_only
                        when ('1', '0001') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_doubling_accum_sisd // SQRDMLSH_asisdsame2_only
                when ('01x1', _, '0x', 'x100', '00xxxxx10', _) => // asisdmisc
                    __field U 29 +: 1
                    __field size 22 +: 2
                    __field opcode 12 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, size, opcode) of
                        when (_, _, '0000x') => __UNALLOCATED
                        when (_, _, '00010') => __UNALLOCATED
                        when (_, _, '0010x') => __UNALLOCATED
                        when (_, _, '00110') => __UNALLOCATED
                        when (_, _, '01111') => __UNALLOCATED
                        when (_, _, '1000x') => __UNALLOCATED
                        when (_, _, '10011') => __UNALLOCATED
                        when (_, _, '10101') => __UNALLOCATED
                        when (_, _, '10111') => __UNALLOCATED
                        when (_, _, '1100x') => __UNALLOCATED
                        when (_, _, '11110') => __UNALLOCATED
                        when (_, '0x', '011xx') => __UNALLOCATED
                        when (_, '0x', '11111') => __UNALLOCATED
                        when (_, '1x', '10110') => __UNALLOCATED
                        when (_, '1x', '11100') => __UNALLOCATED
                        when ('0', _, '00011') => __encoding aarch64_vector_arithmetic_unary_add_saturating_sisd // SUQADD_asisdmisc_R
                        when ('0', _, '00111') => __encoding aarch64_vector_arithmetic_unary_diff_neg_sat_sisd // SQABS_asisdmisc_R
                        when ('0', _, '01000') => __encoding aarch64_vector_arithmetic_unary_cmp_int_bulk_sisd // CMGT_asisdmisc_Z
                        when ('0', _, '01001') => __encoding aarch64_vector_arithmetic_unary_cmp_int_bulk_sisd // CMEQ_asisdmisc_Z
                        when ('0', _, '01010') => __encoding aarch64_vector_arithmetic_unary_cmp_int_lessthan_sisd // CMLT_asisdmisc_Z
                        when ('0', _, '01011') => __encoding aarch64_vector_arithmetic_unary_diff_neg_int_sisd // ABS_asisdmisc_R
                        when ('0', _, '10010') => __UNALLOCATED
                        when ('0', _, '10100') => __encoding aarch64_vector_arithmetic_unary_extract_sat_sisd // SQXTN_asisdmisc_N
                        when ('0', '0x', '10110') => __UNALLOCATED
                        when ('0', '0x', '11010') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_sisd // FCVTNS_asisdmisc_R
                        when ('0', '0x', '11011') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_sisd // FCVTMS_asisdmisc_R
                        when ('0', '0x', '11100') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_tieaway_sisd // FCVTAS_asisdmisc_R
                        when ('0', '0x', '11101') => __encoding aarch64_vector_arithmetic_unary_float_conv_int_sisd // SCVTF_asisdmisc_R
                        when ('0', '1x', '01100') => __encoding aarch64_vector_arithmetic_unary_cmp_float_bulk_sisd // FCMGT_asisdmisc_FZ
                        when ('0', '1x', '01101') => __encoding aarch64_vector_arithmetic_unary_cmp_float_bulk_sisd // FCMEQ_asisdmisc_FZ
                        when ('0', '1x', '01110') => __encoding aarch64_vector_arithmetic_unary_cmp_float_lessthan_sisd // FCMLT_asisdmisc_FZ
                        when ('0', '1x', '11010') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_sisd // FCVTPS_asisdmisc_R
                        when ('0', '1x', '11011') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_sisd // FCVTZS_asisdmisc_R
                        when ('0', '1x', '11101') => __encoding aarch64_vector_arithmetic_unary_special_recip_float_sisd // FRECPE_asisdmisc_R
                        when ('0', '1x', '11111') => __encoding aarch64_vector_arithmetic_unary_special_frecpx // FRECPX_asisdmisc_R
                        when ('1', _, '00011') => __encoding aarch64_vector_arithmetic_unary_add_saturating_sisd // USQADD_asisdmisc_R
                        when ('1', _, '00111') => __encoding aarch64_vector_arithmetic_unary_diff_neg_sat_sisd // SQNEG_asisdmisc_R
                        when ('1', _, '01000') => __encoding aarch64_vector_arithmetic_unary_cmp_int_bulk_sisd // CMGE_asisdmisc_Z
                        when ('1', _, '01001') => __encoding aarch64_vector_arithmetic_unary_cmp_int_bulk_sisd // CMLE_asisdmisc_Z
                        when ('1', _, '01010') => __UNALLOCATED
                        when ('1', _, '01011') => __encoding aarch64_vector_arithmetic_unary_diff_neg_int_sisd // NEG_asisdmisc_R
                        when ('1', _, '10010') => __encoding aarch64_vector_arithmetic_unary_extract_sqxtun_sisd // SQXTUN_asisdmisc_N
                        when ('1', _, '10100') => __encoding aarch64_vector_arithmetic_unary_extract_sat_sisd // UQXTN_asisdmisc_N
                        when ('1', '0x', '10110') => __encoding aarch64_vector_arithmetic_unary_float_xtn_sisd // FCVTXN_asisdmisc_N
                        when ('1', '0x', '11010') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_sisd // FCVTNU_asisdmisc_R
                        when ('1', '0x', '11011') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_sisd // FCVTMU_asisdmisc_R
                        when ('1', '0x', '11100') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_tieaway_sisd // FCVTAU_asisdmisc_R
                        when ('1', '0x', '11101') => __encoding aarch64_vector_arithmetic_unary_float_conv_int_sisd // UCVTF_asisdmisc_R
                        when ('1', '1x', '01100') => __encoding aarch64_vector_arithmetic_unary_cmp_float_bulk_sisd // FCMGE_asisdmisc_FZ
                        when ('1', '1x', '01101') => __encoding aarch64_vector_arithmetic_unary_cmp_float_bulk_sisd // FCMLE_asisdmisc_FZ
                        when ('1', '1x', '01110') => __UNALLOCATED
                        when ('1', '1x', '11010') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_sisd // FCVTPU_asisdmisc_R
                        when ('1', '1x', '11011') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_sisd // FCVTZU_asisdmisc_R
                        when ('1', '1x', '11101') => __encoding aarch64_vector_arithmetic_unary_special_sqrt_est_float_sisd // FRSQRTE_asisdmisc_R
                        when ('1', '1x', '11111') => __UNALLOCATED
                when ('01x1', _, '0x', 'x110', '00xxxxx10', _) => // asisdpair
                    __field U 29 +: 1
                    __field size 22 +: 2
                    __field opcode 12 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, size, opcode) of
                        when (_, _, '00xxx') => __UNALLOCATED
                        when (_, _, '010xx') => __UNALLOCATED
                        when (_, _, '01110') => __UNALLOCATED
                        when (_, _, '10xxx') => __UNALLOCATED
                        when (_, _, '1100x') => __UNALLOCATED
                        when (_, _, '11010') => __UNALLOCATED
                        when (_, _, '111xx') => __UNALLOCATED
                        when (_, '1x', '01101') => __UNALLOCATED
                        when ('0', _, '11011') => __encoding aarch64_vector_reduce_add_sisd // ADDP_asisdpair_only
                        when ('0', '00', '01100') => __encoding aarch64_vector_reduce_fp16_maxnm_sisd // FMAXNMP_asisdpair_only_H
                        when ('0', '00', '01101') => __encoding aarch64_vector_reduce_fp16_add_sisd // FADDP_asisdpair_only_H
                        when ('0', '00', '01111') => __encoding aarch64_vector_reduce_fp16_max_sisd // FMAXP_asisdpair_only_H
                        when ('0', '01', '01100') => __UNALLOCATED
                        when ('0', '01', '01101') => __UNALLOCATED
                        when ('0', '01', '01111') => __UNALLOCATED
                        when ('0', '10', '01100') => __encoding aarch64_vector_reduce_fp16_maxnm_sisd // FMINNMP_asisdpair_only_H
                        when ('0', '10', '01111') => __encoding aarch64_vector_reduce_fp16_max_sisd // FMINP_asisdpair_only_H
                        when ('0', '11', '01100') => __UNALLOCATED
                        when ('0', '11', '01111') => __UNALLOCATED
                        when ('1', _, '11011') => __UNALLOCATED
                        when ('1', '0x', '01100') => __encoding aarch64_vector_reduce_fp_maxnm_sisd // FMAXNMP_asisdpair_only_SD
                        when ('1', '0x', '01101') => __encoding aarch64_vector_reduce_fp_add_sisd // FADDP_asisdpair_only_SD
                        when ('1', '0x', '01111') => __encoding aarch64_vector_reduce_fp_max_sisd // FMAXP_asisdpair_only_SD
                        when ('1', '1x', '01100') => __encoding aarch64_vector_reduce_fp_maxnm_sisd // FMINNMP_asisdpair_only_SD
                        when ('1', '1x', '01111') => __encoding aarch64_vector_reduce_fp_max_sisd // FMINP_asisdpair_only_SD
                when ('01x1', _, '0x', 'x1xx', '1xxxxxx10', _) => __UNPREDICTABLE
                when ('01x1', _, '0x', 'x1xx', 'x1xxxxx10', _) => __UNPREDICTABLE
                when ('01x1', _, '0x', 'x1xx', 'xxxxxxx00', _) => // asisddiff
                    __field U 29 +: 1
                    __field size 22 +: 2
                    __field Rm 16 +: 5
                    __field opcode 12 +: 4
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, opcode) of
                        when (_, '00xx') => __UNALLOCATED
                        when (_, '01xx') => __UNALLOCATED
                        when (_, '1000') => __UNALLOCATED
                        when (_, '1010') => __UNALLOCATED
                        when (_, '1100') => __UNALLOCATED
                        when (_, '111x') => __UNALLOCATED
                        when ('0', '1001') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_dmacc_sisd // SQDMLAL_asisddiff_only
                        when ('0', '1011') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_dmacc_sisd // SQDMLSL_asisddiff_only
                        when ('0', '1101') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_double_sisd // SQDMULL_asisddiff_only
                        when ('1', '1001') => __UNALLOCATED
                        when ('1', '1011') => __UNALLOCATED
                        when ('1', '1101') => __UNALLOCATED
                when ('01x1', _, '0x', 'x1xx', 'xxxxxxxx1', _) => // asisdsame
                    __field U 29 +: 1
                    __field size 22 +: 2
                    __field Rm 16 +: 5
                    __field opcode 11 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, size, opcode) of
                        when (_, _, '00000') => __UNALLOCATED
                        when (_, _, '0001x') => __UNALLOCATED
                        when (_, _, '00100') => __UNALLOCATED
                        when (_, _, '011xx') => __UNALLOCATED
                        when (_, _, '1001x') => __UNALLOCATED
                        when (_, '1x', '11011') => __UNALLOCATED
                        when ('0', _, '00001') => __encoding aarch64_vector_arithmetic_binary_uniform_add_saturating_sisd // SQADD_asisdsame_only
                        when ('0', _, '00101') => __encoding aarch64_vector_arithmetic_binary_uniform_sub_saturating_sisd // SQSUB_asisdsame_only
                        when ('0', _, '00110') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_int_sisd // CMGT_asisdsame_only
                        when ('0', _, '00111') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_int_sisd // CMGE_asisdsame_only
                        when ('0', _, '01000') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_sisd // SSHL_asisdsame_only
                        when ('0', _, '01001') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_sisd // SQSHL_asisdsame_only
                        when ('0', _, '01010') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_sisd // SRSHL_asisdsame_only
                        when ('0', _, '01011') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_sisd // SQRSHL_asisdsame_only
                        when ('0', _, '10000') => __encoding aarch64_vector_arithmetic_binary_uniform_add_wrapping_single_sisd // ADD_asisdsame_only
                        when ('0', _, '10001') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_bitwise_sisd // CMTST_asisdsame_only
                        when ('0', _, '10100') => __UNALLOCATED
                        when ('0', _, '10101') => __UNALLOCATED
                        when ('0', _, '10110') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_doubling_sisd // SQDMULH_asisdsame_only
                        when ('0', _, '10111') => __UNALLOCATED
                        when ('0', '0x', '11000') => __UNALLOCATED
                        when ('0', '0x', '11001') => __UNALLOCATED
                        when ('0', '0x', '11010') => __UNALLOCATED
                        when ('0', '0x', '11011') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp_extended_sisd // FMULX_asisdsame_only
                        when ('0', '0x', '11100') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp_sisd // FCMEQ_asisdsame_only
                        when ('0', '0x', '11101') => __UNALLOCATED
                        when ('0', '0x', '11110') => __UNALLOCATED
                        when ('0', '0x', '11111') => __encoding aarch64_vector_arithmetic_binary_uniform_recps_sisd // FRECPS_asisdsame_only
                        when ('0', '1x', '11000') => __UNALLOCATED
                        when ('0', '1x', '11001') => __UNALLOCATED
                        when ('0', '1x', '11010') => __UNALLOCATED
                        when ('0', '1x', '11100') => __UNALLOCATED
                        when ('0', '1x', '11101') => __UNALLOCATED
                        when ('0', '1x', '11110') => __UNALLOCATED
                        when ('0', '1x', '11111') => __encoding aarch64_vector_arithmetic_binary_uniform_rsqrts_sisd // FRSQRTS_asisdsame_only
                        when ('1', _, '00001') => __encoding aarch64_vector_arithmetic_binary_uniform_add_saturating_sisd // UQADD_asisdsame_only
                        when ('1', _, '00101') => __encoding aarch64_vector_arithmetic_binary_uniform_sub_saturating_sisd // UQSUB_asisdsame_only
                        when ('1', _, '00110') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_int_sisd // CMHI_asisdsame_only
                        when ('1', _, '00111') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_int_sisd // CMHS_asisdsame_only
                        when ('1', _, '01000') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_sisd // USHL_asisdsame_only
                        when ('1', _, '01001') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_sisd // UQSHL_asisdsame_only
                        when ('1', _, '01010') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_sisd // URSHL_asisdsame_only
                        when ('1', _, '01011') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_sisd // UQRSHL_asisdsame_only
                        when ('1', _, '10000') => __encoding aarch64_vector_arithmetic_binary_uniform_add_wrapping_single_sisd // SUB_asisdsame_only
                        when ('1', _, '10001') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_bitwise_sisd // CMEQ_asisdsame_only
                        when ('1', _, '10100') => __UNALLOCATED
                        when ('1', _, '10101') => __UNALLOCATED
                        when ('1', _, '10110') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_doubling_sisd // SQRDMULH_asisdsame_only
                        when ('1', _, '10111') => __UNALLOCATED
                        when ('1', '0x', '11000') => __UNALLOCATED
                        when ('1', '0x', '11001') => __UNALLOCATED
                        when ('1', '0x', '11010') => __UNALLOCATED
                        when ('1', '0x', '11011') => __UNALLOCATED
                        when ('1', '0x', '11100') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp_sisd // FCMGE_asisdsame_only
                        when ('1', '0x', '11101') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp_sisd // FACGE_asisdsame_only
                        when ('1', '0x', '11110') => __UNALLOCATED
                        when ('1', '0x', '11111') => __UNALLOCATED
                        when ('1', '1x', '11000') => __UNALLOCATED
                        when ('1', '1x', '11001') => __UNALLOCATED
                        when ('1', '1x', '11010') => __encoding aarch64_vector_arithmetic_binary_uniform_sub_fp_sisd // FABD_asisdsame_only
                        when ('1', '1x', '11100') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp_sisd // FCMGT_asisdsame_only
                        when ('1', '1x', '11101') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp_sisd // FACGT_asisdsame_only
                        when ('1', '1x', '11110') => __UNALLOCATED
                        when ('1', '1x', '11111') => __UNALLOCATED
                when ('01x1', _, '10', _, 'xxxxxxxx1', _) => // asisdshf
                    __field U 29 +: 1
                    __field immh 19 +: 4
                    __field immb 16 +: 3
                    __field opcode 11 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, immh, opcode) of
                        when (_, !'0000', '00001') => __UNALLOCATED
                        when (_, !'0000', '00011') => __UNALLOCATED
                        when (_, !'0000', '00101') => __UNALLOCATED
                        when (_, !'0000', '00111') => __UNALLOCATED
                        when (_, !'0000', '01001') => __UNALLOCATED
                        when (_, !'0000', '01011') => __UNALLOCATED
                        when (_, !'0000', '01101') => __UNALLOCATED
                        when (_, !'0000', '01111') => __UNALLOCATED
                        when (_, !'0000', '101xx') => __UNALLOCATED
                        when (_, !'0000', '110xx') => __UNALLOCATED
                        when (_, !'0000', '11101') => __UNALLOCATED
                        when (_, !'0000', '11110') => __UNALLOCATED
                        when (_, '0000', _) => __UNALLOCATED
                        when ('0', !'0000', '00000') => __encoding aarch64_vector_shift_right_sisd // SSHR_asisdshf_R
                        when ('0', !'0000', '00010') => __encoding aarch64_vector_shift_right_sisd // SSRA_asisdshf_R
                        when ('0', !'0000', '00100') => __encoding aarch64_vector_shift_right_sisd // SRSHR_asisdshf_R
                        when ('0', !'0000', '00110') => __encoding aarch64_vector_shift_right_sisd // SRSRA_asisdshf_R
                        when ('0', !'0000', '01000') => __UNALLOCATED
                        when ('0', !'0000', '01010') => __encoding aarch64_vector_shift_left_sisd // SHL_asisdshf_R
                        when ('0', !'0000', '01100') => __UNALLOCATED
                        when ('0', !'0000', '01110') => __encoding aarch64_vector_shift_left_sat_sisd // SQSHL_asisdshf_R
                        when ('0', !'0000', '10000') => __UNALLOCATED
                        when ('0', !'0000', '10001') => __UNALLOCATED
                        when ('0', !'0000', '10010') => __encoding aarch64_vector_shift_right_narrow_uniform_sisd // SQSHRN_asisdshf_N
                        when ('0', !'0000', '10011') => __encoding aarch64_vector_shift_right_narrow_uniform_sisd // SQRSHRN_asisdshf_N
                        when ('0', !'0000', '11100') => __encoding aarch64_vector_shift_conv_int_sisd // SCVTF_asisdshf_C
                        when ('0', !'0000', '11111') => __encoding aarch64_vector_shift_conv_float_sisd // FCVTZS_asisdshf_C
                        when ('1', !'0000', '00000') => __encoding aarch64_vector_shift_right_sisd // USHR_asisdshf_R
                        when ('1', !'0000', '00010') => __encoding aarch64_vector_shift_right_sisd // USRA_asisdshf_R
                        when ('1', !'0000', '00100') => __encoding aarch64_vector_shift_right_sisd // URSHR_asisdshf_R
                        when ('1', !'0000', '00110') => __encoding aarch64_vector_shift_right_sisd // URSRA_asisdshf_R
                        when ('1', !'0000', '01000') => __encoding aarch64_vector_shift_right_insert_sisd // SRI_asisdshf_R
                        when ('1', !'0000', '01010') => __encoding aarch64_vector_shift_left_insert_sisd // SLI_asisdshf_R
                        when ('1', !'0000', '01100') => __encoding aarch64_vector_shift_left_sat_sisd // SQSHLU_asisdshf_R
                        when ('1', !'0000', '01110') => __encoding aarch64_vector_shift_left_sat_sisd // UQSHL_asisdshf_R
                        when ('1', !'0000', '10000') => __encoding aarch64_vector_shift_right_narrow_nonuniform_sisd // SQSHRUN_asisdshf_N
                        when ('1', !'0000', '10001') => __encoding aarch64_vector_shift_right_narrow_nonuniform_sisd // SQRSHRUN_asisdshf_N
                        when ('1', !'0000', '10010') => __encoding aarch64_vector_shift_right_narrow_uniform_sisd // UQSHRN_asisdshf_N
                        when ('1', !'0000', '10011') => __encoding aarch64_vector_shift_right_narrow_uniform_sisd // UQRSHRN_asisdshf_N
                        when ('1', !'0000', '11100') => __encoding aarch64_vector_shift_conv_int_sisd // UCVTF_asisdshf_C
                        when ('1', !'0000', '11111') => __encoding aarch64_vector_shift_conv_float_sisd // FCVTZU_asisdshf_C
                when ('01x1', _, '11', _, 'xxxxxxxx1', _) => __UNPREDICTABLE
                when ('01x1', _, '1x', _, 'xxxxxxxx0', _) => // asisdelem
                    __field U 29 +: 1
                    __field size 22 +: 2
                    __field L 21 +: 1
                    __field M 20 +: 1
                    __field Rm 16 +: 4
                    __field opcode 12 +: 4
                    __field H 11 +: 1
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, size, opcode) of
                        when (_, _, '0000') => __UNALLOCATED
                        when (_, _, '0010') => __UNALLOCATED
                        when (_, _, '0100') => __UNALLOCATED
                        when (_, _, '0110') => __UNALLOCATED
                        when (_, _, '1000') => __UNALLOCATED
                        when (_, _, '1010') => __UNALLOCATED
                        when (_, _, '1110') => __UNALLOCATED
                        when (_, '01', '0001') => __UNALLOCATED
                        when (_, '01', '0101') => __UNALLOCATED
                        when (_, '01', '1001') => __UNALLOCATED
                        when ('0', _, '0011') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_double_sisd // SQDMLAL_asisdelem_L
                        when ('0', _, '0111') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_double_sisd // SQDMLSL_asisdelem_L
                        when ('0', _, '1011') => __encoding aarch64_vector_arithmetic_binary_element_mul_double_sisd // SQDMULL_asisdelem_L
                        when ('0', _, '1100') => __encoding aarch64_vector_arithmetic_binary_element_mul_high_sisd // SQDMULH_asisdelem_R
                        when ('0', _, '1101') => __encoding aarch64_vector_arithmetic_binary_element_mul_high_sisd // SQRDMULH_asisdelem_R
                        when ('0', _, '1111') => __UNALLOCATED
                        when ('0', '00', '0001') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_fp16_sisd // FMLA_asisdelem_RH_H
                        when ('0', '00', '0101') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_fp16_sisd // FMLS_asisdelem_RH_H
                        when ('0', '00', '1001') => __encoding aarch64_vector_arithmetic_binary_element_mul_fp16_sisd // FMUL_asisdelem_RH_H
                        when ('0', '1x', '0001') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_fp_sisd // FMLA_asisdelem_R_SD
                        when ('0', '1x', '0101') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_fp_sisd // FMLS_asisdelem_R_SD
                        when ('0', '1x', '1001') => __encoding aarch64_vector_arithmetic_binary_element_mul_fp_sisd // FMUL_asisdelem_R_SD
                        when ('1', _, '0011') => __UNALLOCATED
                        when ('1', _, '0111') => __UNALLOCATED
                        when ('1', _, '1011') => __UNALLOCATED
                        when ('1', _, '1100') => __UNALLOCATED
                        when ('1', _, '1101') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_high_sisd // SQRDMLAH_asisdelem_R
                        when ('1', _, '1111') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_high_sisd // SQRDMLSH_asisdelem_R
                        when ('1', '00', '0001') => __UNALLOCATED
                        when ('1', '00', '0101') => __UNALLOCATED
                        when ('1', '00', '1001') => __encoding aarch64_vector_arithmetic_binary_element_mul_fp16_sisd // FMULX_asisdelem_RH_H
                        when ('1', '1x', '0001') => __UNALLOCATED
                        when ('1', '1x', '0101') => __UNALLOCATED
                        when ('1', '1x', '1001') => __encoding aarch64_vector_arithmetic_binary_element_mul_fp_sisd // FMULX_asisdelem_R_SD
                when ('0x00', _, '0x', 'x0xx', 'xxx0xxx00', _) => // asimdtbl
                    __field Q 30 +: 1
                    __field op2 22 +: 2
                    __field Rm 16 +: 5
                    __field len 13 +: 2
                    __field op 12 +: 1
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (op2, len, op) of
                        when ('x1', _, _) => __UNALLOCATED
                        when ('00', '00', '0') => __encoding aarch64_vector_transfer_vector_table // TBL_asimdtbl_L1_1
                        when ('00', '00', '1') => __encoding aarch64_vector_transfer_vector_table // TBX_asimdtbl_L1_1
                        when ('00', '01', '0') => __encoding aarch64_vector_transfer_vector_table // TBL_asimdtbl_L2_2
                        when ('00', '01', '1') => __encoding aarch64_vector_transfer_vector_table // TBX_asimdtbl_L2_2
                        when ('00', '10', '0') => __encoding aarch64_vector_transfer_vector_table // TBL_asimdtbl_L3_3
                        when ('00', '10', '1') => __encoding aarch64_vector_transfer_vector_table // TBX_asimdtbl_L3_3
                        when ('00', '11', '0') => __encoding aarch64_vector_transfer_vector_table // TBL_asimdtbl_L4_4
                        when ('00', '11', '1') => __encoding aarch64_vector_transfer_vector_table // TBX_asimdtbl_L4_4
                        when ('1x', _, _) => __UNALLOCATED
                when ('0x00', _, '0x', 'x0xx', 'xxx0xxx10', _) => // asimdperm
                    __field Q 30 +: 1
                    __field size 22 +: 2
                    __field Rm 16 +: 5
                    __field opcode 12 +: 3
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (opcode) of
                        when ('000') => __UNALLOCATED
                        when ('001') => __encoding aarch64_vector_transfer_vector_permute_unzip // UZP1_asimdperm_only
                        when ('010') => __encoding aarch64_vector_transfer_vector_permute_transpose // TRN1_asimdperm_only
                        when ('011') => __encoding aarch64_vector_transfer_vector_permute_zip // ZIP1_asimdperm_only
                        when ('100') => __UNALLOCATED
                        when ('101') => __encoding aarch64_vector_transfer_vector_permute_unzip // UZP2_asimdperm_only
                        when ('110') => __encoding aarch64_vector_transfer_vector_permute_transpose // TRN2_asimdperm_only
                        when ('111') => __encoding aarch64_vector_transfer_vector_permute_zip // ZIP2_asimdperm_only
                when ('0x10', _, '0x', 'x0xx', 'xxx0xxxx0', _) => // asimdext
                    __field Q 30 +: 1
                    __field op2 22 +: 2
                    __field Rm 16 +: 5
                    __field imm4 11 +: 4
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (op2) of
                        when ('x1') => __UNALLOCATED
                        when ('00') => __encoding aarch64_vector_transfer_vector_extract // EXT_asimdext_only
                        when ('1x') => __UNALLOCATED
                when ('0xx0', _, '00', '00xx', 'xxx0xxxx1', _) => // asimdins
                    __field Q 30 +: 1
                    __field op 29 +: 1
                    __field imm5 16 +: 5
                    __field imm4 11 +: 4
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (Q, op, imm5, imm4) of
                        when (_, _, 'x0000', _) => __UNALLOCATED
                        when (_, '0', _, '0000') => __encoding aarch64_vector_transfer_vector_cpy_dup_simd // DUP_asimdins_DV_v
                        when (_, '0', _, '0001') => __encoding aarch64_vector_transfer_integer_dup // DUP_asimdins_DR_r
                        when (_, '0', _, '0010') => __UNALLOCATED
                        when (_, '0', _, '0100') => __UNALLOCATED
                        when (_, '0', _, '0110') => __UNALLOCATED
                        when (_, '0', _, '1xxx') => __UNALLOCATED
                        when ('0', '0', _, '0011') => __UNALLOCATED
                        when ('0', '0', _, '0101') => __encoding aarch64_vector_transfer_integer_move_signed // SMOV_asimdins_W_w
                        when ('0', '0', _, '0111') => __encoding aarch64_vector_transfer_integer_move_unsigned // UMOV_asimdins_W_w
                        when ('0', '1', _, _) => __UNALLOCATED
                        when ('1', '0', _, '0011') => __encoding aarch64_vector_transfer_integer_insert // INS_asimdins_IR_r
                        when ('1', '0', _, '0101') => __encoding aarch64_vector_transfer_integer_move_signed // SMOV_asimdins_X_x
                        when ('1', '0', 'x1000', '0111') => __encoding aarch64_vector_transfer_integer_move_unsigned // UMOV_asimdins_X_x
                        when ('1', '1', _, _) => __encoding aarch64_vector_transfer_vector_insert // INS_asimdins_IV_v
                when ('0xx0', _, '01', '00xx', 'xxx0xxxx1', _) => __UNPREDICTABLE
                when ('0xx0', _, '0x', '0111', '00xxxxx10', _) => __UNPREDICTABLE
                when ('0xx0', _, '0x', '10xx', 'xxx00xxx1', _) => // asimdsamefp16
                    __field Q 30 +: 1
                    __field U 29 +: 1
                    __field a 23 +: 1
                    __field Rm 16 +: 5
                    __field opcode 11 +: 3
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, a, opcode) of
                        when ('0', '0', '000') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp16_2008 // FMAXNM_asimdsamefp16_only
                        when ('0', '0', '001') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp16_fused // FMLA_asimdsamefp16_only
                        when ('0', '0', '010') => __encoding aarch64_vector_arithmetic_binary_uniform_add_fp16 // FADD_asimdsamefp16_only
                        when ('0', '0', '011') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp16_extended_simd // FMULX_asimdsamefp16_only
                        when ('0', '0', '100') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp16_simd // FCMEQ_asimdsamefp16_only
                        when ('0', '0', '101') => __UNALLOCATED
                        when ('0', '0', '110') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp16_1985 // FMAX_asimdsamefp16_only
                        when ('0', '0', '111') => __encoding aarch64_vector_arithmetic_binary_uniform_recps_fp16_simd // FRECPS_asimdsamefp16_only
                        when ('0', '1', '000') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp16_2008 // FMINNM_asimdsamefp16_only
                        when ('0', '1', '001') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp16_fused // FMLS_asimdsamefp16_only
                        when ('0', '1', '010') => __encoding aarch64_vector_arithmetic_binary_uniform_sub_fp16_simd // FSUB_asimdsamefp16_only
                        when ('0', '1', '011') => __UNALLOCATED
                        when ('0', '1', '100') => __UNALLOCATED
                        when ('0', '1', '101') => __UNALLOCATED
                        when ('0', '1', '110') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp16_1985 // FMIN_asimdsamefp16_only
                        when ('0', '1', '111') => __encoding aarch64_vector_arithmetic_binary_uniform_rsqrts_fp16_simd // FRSQRTS_asimdsamefp16_only
                        when ('1', '0', '000') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp16_2008 // FMAXNMP_asimdsamefp16_only
                        when ('1', '0', '001') => __UNALLOCATED
                        when ('1', '0', '010') => __encoding aarch64_vector_arithmetic_binary_uniform_add_fp16 // FADDP_asimdsamefp16_only
                        when ('1', '0', '011') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp16_product // FMUL_asimdsamefp16_only
                        when ('1', '0', '100') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp16_simd // FCMGE_asimdsamefp16_only
                        when ('1', '0', '101') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp16_simd // FACGE_asimdsamefp16_only
                        when ('1', '0', '110') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp16_1985 // FMAXP_asimdsamefp16_only
                        when ('1', '0', '111') => __encoding aarch64_vector_arithmetic_binary_uniform_div_fp16 // FDIV_asimdsamefp16_only
                        when ('1', '1', '000') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp16_2008 // FMINNMP_asimdsamefp16_only
                        when ('1', '1', '001') => __UNALLOCATED
                        when ('1', '1', '010') => __encoding aarch64_vector_arithmetic_binary_uniform_sub_fp16_simd // FABD_asimdsamefp16_only
                        when ('1', '1', '011') => __UNALLOCATED
                        when ('1', '1', '100') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp16_simd // FCMGT_asimdsamefp16_only
                        when ('1', '1', '101') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp16_simd // FACGT_asimdsamefp16_only
                        when ('1', '1', '110') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp16_1985 // FMINP_asimdsamefp16_only
                        when ('1', '1', '111') => __UNALLOCATED
                when ('0xx0', _, '0x', '10xx', 'xxx01xxx1', _) => __UNPREDICTABLE
                when ('0xx0', _, '0x', '1111', '00xxxxx10', _) => // asimdmiscfp16
                    __field Q 30 +: 1
                    __field U 29 +: 1
                    __field a 23 +: 1
                    __field opcode 12 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, a, opcode) of
                        when (_, _, '00xxx') => __UNALLOCATED
                        when (_, _, '010xx') => __UNALLOCATED
                        when (_, _, '10xxx') => __UNALLOCATED
                        when (_, _, '11110') => __UNALLOCATED
                        when (_, '0', '011xx') => __UNALLOCATED
                        when (_, '0', '11111') => __UNALLOCATED
                        when (_, '1', '11100') => __UNALLOCATED
                        when ('0', '0', '11000') => __encoding aarch64_vector_arithmetic_unary_fp16_round // FRINTN_asimdmiscfp16_R
                        when ('0', '0', '11001') => __encoding aarch64_vector_arithmetic_unary_fp16_round // FRINTM_asimdmiscfp16_R
                        when ('0', '0', '11010') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_simd // FCVTNS_asimdmiscfp16_R
                        when ('0', '0', '11011') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_simd // FCVTMS_asimdmiscfp16_R
                        when ('0', '0', '11100') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_tieaway_simd // FCVTAS_asimdmiscfp16_R
                        when ('0', '0', '11101') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_int_simd // SCVTF_asimdmiscfp16_R
                        when ('0', '1', '01100') => __encoding aarch64_vector_arithmetic_unary_cmp_fp16_bulk_simd // FCMGT_asimdmiscfp16_FZ
                        when ('0', '1', '01101') => __encoding aarch64_vector_arithmetic_unary_cmp_fp16_bulk_simd // FCMEQ_asimdmiscfp16_FZ
                        when ('0', '1', '01110') => __encoding aarch64_vector_arithmetic_unary_cmp_fp16_lessthan_simd // FCMLT_asimdmiscfp16_FZ
                        when ('0', '1', '01111') => __encoding aarch64_vector_arithmetic_unary_diff_neg_fp16 // FABS_asimdmiscfp16_R
                        when ('0', '1', '11000') => __encoding aarch64_vector_arithmetic_unary_fp16_round // FRINTP_asimdmiscfp16_R
                        when ('0', '1', '11001') => __encoding aarch64_vector_arithmetic_unary_fp16_round // FRINTZ_asimdmiscfp16_R
                        when ('0', '1', '11010') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_simd // FCVTPS_asimdmiscfp16_R
                        when ('0', '1', '11011') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_simd // FCVTZS_asimdmiscfp16_R
                        when ('0', '1', '11101') => __encoding aarch64_vector_arithmetic_unary_special_recip_fp16_simd // FRECPE_asimdmiscfp16_R
                        when ('0', '1', '11111') => __UNALLOCATED
                        when ('1', '0', '11000') => __encoding aarch64_vector_arithmetic_unary_fp16_round // FRINTA_asimdmiscfp16_R
                        when ('1', '0', '11001') => __encoding aarch64_vector_arithmetic_unary_fp16_round // FRINTX_asimdmiscfp16_R
                        when ('1', '0', '11010') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_simd // FCVTNU_asimdmiscfp16_R
                        when ('1', '0', '11011') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_simd // FCVTMU_asimdmiscfp16_R
                        when ('1', '0', '11100') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_tieaway_simd // FCVTAU_asimdmiscfp16_R
                        when ('1', '0', '11101') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_int_simd // UCVTF_asimdmiscfp16_R
                        when ('1', '1', '01100') => __encoding aarch64_vector_arithmetic_unary_cmp_fp16_bulk_simd // FCMGE_asimdmiscfp16_FZ
                        when ('1', '1', '01101') => __encoding aarch64_vector_arithmetic_unary_cmp_fp16_bulk_simd // FCMLE_asimdmiscfp16_FZ
                        when ('1', '1', '01110') => __UNALLOCATED
                        when ('1', '1', '01111') => __encoding aarch64_vector_arithmetic_unary_diff_neg_fp16 // FNEG_asimdmiscfp16_R
                        when ('1', '1', '11000') => __UNALLOCATED
                        when ('1', '1', '11001') => __encoding aarch64_vector_arithmetic_unary_fp16_round // FRINTI_asimdmiscfp16_R
                        when ('1', '1', '11010') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_simd // FCVTPU_asimdmiscfp16_R
                        when ('1', '1', '11011') => __encoding aarch64_vector_arithmetic_unary_fp16_conv_float_bulk_simd // FCVTZU_asimdmiscfp16_R
                        when ('1', '1', '11101') => __encoding aarch64_vector_arithmetic_unary_special_sqrt_est_fp16_simd // FRSQRTE_asimdmiscfp16_R
                        when ('1', '1', '11111') => __encoding aarch64_vector_arithmetic_unary_special_sqrt_fp16 // FSQRT_asimdmiscfp16_R
                when ('0xx0', _, '0x', 'x0xx', 'xxx1xxxx0', _) => __UNPREDICTABLE
                when ('0xx0', _, '0x', 'x0xx', 'xxx1xxxx1', _) => // asimdsame2
                    __field Q 30 +: 1
                    __field U 29 +: 1
                    __field size 22 +: 2
                    __field Rm 16 +: 5
                    __field opcode 11 +: 4
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, size, opcode) of
                        when (_, _, '0011') => __UNALLOCATED
                        when (_, _, '01xx') => __UNALLOCATED
                        when ('0', _, '0000') => __UNALLOCATED
                        when ('0', _, '0001') => __UNALLOCATED
                        when ('0', _, '0010') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_dotp // SDOT_asimdsame2_D
                        when ('0', _, '1xxx') => __UNALLOCATED
                        when ('1', _, '0000') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_doubling_accum_simd // SQRDMLAH_asimdsame2_only
                        when ('1', _, '0001') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_doubling_accum_simd // SQRDMLSH_asimdsame2_only
                        when ('1', _, '0010') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_dotp // UDOT_asimdsame2_D
                        when ('1', _, '10xx') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp_complex // FCMLA_asimdsame2_C
                        when ('1', _, '11x0') => __encoding aarch64_vector_arithmetic_binary_uniform_add_fp_complex // FCADD_asimdsame2_C
                        when ('1', '00', '1101') => __UNALLOCATED
                        when ('1', '00', '1111') => __UNALLOCATED
                        when ('1', '1x', '1101') => __UNALLOCATED
                        when ('1', '1x', '1111') => __UNALLOCATED
                when ('0xx0', _, '0x', 'x100', '00xxxxx10', _) => // asimdmisc
                    __field Q 30 +: 1
                    __field U 29 +: 1
                    __field size 22 +: 2
                    __field opcode 12 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, size, opcode) of
                        when (_, _, '1000x') => __UNALLOCATED
                        when (_, _, '10101') => __UNALLOCATED
                        when (_, '0x', '011xx') => __UNALLOCATED
                        when (_, '1x', '10111') => __UNALLOCATED
                        when (_, '1x', '11110') => __UNALLOCATED
                        when (_, '11', '10110') => __UNALLOCATED
                        when ('0', _, '00000') => __encoding aarch64_vector_arithmetic_unary_rev // REV64_asimdmisc_R
                        when ('0', _, '00001') => __encoding aarch64_vector_arithmetic_unary_rev // REV16_asimdmisc_R
                        when ('0', _, '00010') => __encoding aarch64_vector_arithmetic_unary_add_pairwise // SADDLP_asimdmisc_P
                        when ('0', _, '00011') => __encoding aarch64_vector_arithmetic_unary_add_saturating_simd // SUQADD_asimdmisc_R
                        when ('0', _, '00100') => __encoding aarch64_vector_arithmetic_unary_clsz // CLS_asimdmisc_R
                        when ('0', _, '00101') => __encoding aarch64_vector_arithmetic_unary_cnt // CNT_asimdmisc_R
                        when ('0', _, '00110') => __encoding aarch64_vector_arithmetic_unary_add_pairwise // SADALP_asimdmisc_P
                        when ('0', _, '00111') => __encoding aarch64_vector_arithmetic_unary_diff_neg_sat_simd // SQABS_asimdmisc_R
                        when ('0', _, '01000') => __encoding aarch64_vector_arithmetic_unary_cmp_int_bulk_simd // CMGT_asimdmisc_Z
                        when ('0', _, '01001') => __encoding aarch64_vector_arithmetic_unary_cmp_int_bulk_simd // CMEQ_asimdmisc_Z
                        when ('0', _, '01010') => __encoding aarch64_vector_arithmetic_unary_cmp_int_lessthan_simd // CMLT_asimdmisc_Z
                        when ('0', _, '01011') => __encoding aarch64_vector_arithmetic_unary_diff_neg_int_simd // ABS_asimdmisc_R
                        when ('0', _, '10010') => __encoding aarch64_vector_arithmetic_unary_extract_nosat // XTN_asimdmisc_N
                        when ('0', _, '10011') => __UNALLOCATED
                        when ('0', _, '10100') => __encoding aarch64_vector_arithmetic_unary_extract_sat_simd // SQXTN_asimdmisc_N
                        when ('0', '0x', '10110') => __encoding aarch64_vector_arithmetic_unary_float_narrow // FCVTN_asimdmisc_N
                        when ('0', '0x', '10111') => __encoding aarch64_vector_arithmetic_unary_float_widen // FCVTL_asimdmisc_L
                        when ('0', '0x', '11000') => __encoding aarch64_vector_arithmetic_unary_float_round // FRINTN_asimdmisc_R
                        when ('0', '0x', '11001') => __encoding aarch64_vector_arithmetic_unary_float_round // FRINTM_asimdmisc_R
                        when ('0', '0x', '11010') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_simd // FCVTNS_asimdmisc_R
                        when ('0', '0x', '11011') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_simd // FCVTMS_asimdmisc_R
                        when ('0', '0x', '11100') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_tieaway_simd // FCVTAS_asimdmisc_R
                        when ('0', '0x', '11101') => __encoding aarch64_vector_arithmetic_unary_float_conv_int_simd // SCVTF_asimdmisc_R
                        when ('0', '0x', '11110') => __encoding aarch64_vector_arithmetic_unary_float_round_frint_32_64 // FRINT32Z_asimdmisc_R
                        when ('0', '0x', '11111') => __encoding aarch64_vector_arithmetic_unary_float_round_frint_32_64 // FRINT64Z_asimdmisc_R
                        when ('0', '1x', '01100') => __encoding aarch64_vector_arithmetic_unary_cmp_float_bulk_simd // FCMGT_asimdmisc_FZ
                        when ('0', '1x', '01101') => __encoding aarch64_vector_arithmetic_unary_cmp_float_bulk_simd // FCMEQ_asimdmisc_FZ
                        when ('0', '1x', '01110') => __encoding aarch64_vector_arithmetic_unary_cmp_float_lessthan_simd // FCMLT_asimdmisc_FZ
                        when ('0', '1x', '01111') => __encoding aarch64_vector_arithmetic_unary_diff_neg_float // FABS_asimdmisc_R
                        when ('0', '1x', '11000') => __encoding aarch64_vector_arithmetic_unary_float_round // FRINTP_asimdmisc_R
                        when ('0', '1x', '11001') => __encoding aarch64_vector_arithmetic_unary_float_round // FRINTZ_asimdmisc_R
                        when ('0', '1x', '11010') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_simd // FCVTPS_asimdmisc_R
                        when ('0', '1x', '11011') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_simd // FCVTZS_asimdmisc_R
                        when ('0', '1x', '11100') => __encoding aarch64_vector_arithmetic_unary_special_recip_int // URECPE_asimdmisc_R
                        when ('0', '1x', '11101') => __encoding aarch64_vector_arithmetic_unary_special_recip_float_simd // FRECPE_asimdmisc_R
                        when ('0', '1x', '11111') => __UNALLOCATED
                        when ('1', _, '00000') => __encoding aarch64_vector_arithmetic_unary_rev // REV32_asimdmisc_R
                        when ('1', _, '00001') => __UNALLOCATED
                        when ('1', _, '00010') => __encoding aarch64_vector_arithmetic_unary_add_pairwise // UADDLP_asimdmisc_P
                        when ('1', _, '00011') => __encoding aarch64_vector_arithmetic_unary_add_saturating_simd // USQADD_asimdmisc_R
                        when ('1', _, '00100') => __encoding aarch64_vector_arithmetic_unary_clsz // CLZ_asimdmisc_R
                        when ('1', _, '00110') => __encoding aarch64_vector_arithmetic_unary_add_pairwise // UADALP_asimdmisc_P
                        when ('1', _, '00111') => __encoding aarch64_vector_arithmetic_unary_diff_neg_sat_simd // SQNEG_asimdmisc_R
                        when ('1', _, '01000') => __encoding aarch64_vector_arithmetic_unary_cmp_int_bulk_simd // CMGE_asimdmisc_Z
                        when ('1', _, '01001') => __encoding aarch64_vector_arithmetic_unary_cmp_int_bulk_simd // CMLE_asimdmisc_Z
                        when ('1', _, '01010') => __UNALLOCATED
                        when ('1', _, '01011') => __encoding aarch64_vector_arithmetic_unary_diff_neg_int_simd // NEG_asimdmisc_R
                        when ('1', _, '10010') => __encoding aarch64_vector_arithmetic_unary_extract_sqxtun_simd // SQXTUN_asimdmisc_N
                        when ('1', _, '10011') => __encoding aarch64_vector_arithmetic_unary_shift // SHLL_asimdmisc_S
                        when ('1', _, '10100') => __encoding aarch64_vector_arithmetic_unary_extract_sat_simd // UQXTN_asimdmisc_N
                        when ('1', '0x', '10110') => __encoding aarch64_vector_arithmetic_unary_float_xtn_simd // FCVTXN_asimdmisc_N
                        when ('1', '0x', '10111') => __UNALLOCATED
                        when ('1', '0x', '11000') => __encoding aarch64_vector_arithmetic_unary_float_round // FRINTA_asimdmisc_R
                        when ('1', '0x', '11001') => __encoding aarch64_vector_arithmetic_unary_float_round // FRINTX_asimdmisc_R
                        when ('1', '0x', '11010') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_simd // FCVTNU_asimdmisc_R
                        when ('1', '0x', '11011') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_simd // FCVTMU_asimdmisc_R
                        when ('1', '0x', '11100') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_tieaway_simd // FCVTAU_asimdmisc_R
                        when ('1', '0x', '11101') => __encoding aarch64_vector_arithmetic_unary_float_conv_int_simd // UCVTF_asimdmisc_R
                        when ('1', '0x', '11110') => __encoding aarch64_vector_arithmetic_unary_float_round_frint_32_64 // FRINT32X_asimdmisc_R
                        when ('1', '0x', '11111') => __encoding aarch64_vector_arithmetic_unary_float_round_frint_32_64 // FRINT64X_asimdmisc_R
                        when ('1', '00', '00101') => __encoding aarch64_vector_arithmetic_unary_not // NOT_asimdmisc_R
                        when ('1', '01', '00101') => __encoding aarch64_vector_arithmetic_unary_rbit // RBIT_asimdmisc_R
                        when ('1', '1x', '00101') => __UNALLOCATED
                        when ('1', '1x', '01100') => __encoding aarch64_vector_arithmetic_unary_cmp_float_bulk_simd // FCMGE_asimdmisc_FZ
                        when ('1', '1x', '01101') => __encoding aarch64_vector_arithmetic_unary_cmp_float_bulk_simd // FCMLE_asimdmisc_FZ
                        when ('1', '1x', '01110') => __UNALLOCATED
                        when ('1', '1x', '01111') => __encoding aarch64_vector_arithmetic_unary_diff_neg_float // FNEG_asimdmisc_R
                        when ('1', '1x', '11000') => __UNALLOCATED
                        when ('1', '1x', '11001') => __encoding aarch64_vector_arithmetic_unary_float_round // FRINTI_asimdmisc_R
                        when ('1', '1x', '11010') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_simd // FCVTPU_asimdmisc_R
                        when ('1', '1x', '11011') => __encoding aarch64_vector_arithmetic_unary_float_conv_float_bulk_simd // FCVTZU_asimdmisc_R
                        when ('1', '1x', '11100') => __encoding aarch64_vector_arithmetic_unary_special_sqrt_est_int // URSQRTE_asimdmisc_R
                        when ('1', '1x', '11101') => __encoding aarch64_vector_arithmetic_unary_special_sqrt_est_float_simd // FRSQRTE_asimdmisc_R
                        when ('1', '1x', '11111') => __encoding aarch64_vector_arithmetic_unary_special_sqrt // FSQRT_asimdmisc_R
                        when ('1', '10', '10110') => __UNALLOCATED
                when ('0xx0', _, '0x', 'x110', '00xxxxx10', _) => // asimdall
                    __field Q 30 +: 1
                    __field U 29 +: 1
                    __field size 22 +: 2
                    __field opcode 12 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, size, opcode) of
                        when (_, _, '0000x') => __UNALLOCATED
                        when (_, _, '00010') => __UNALLOCATED
                        when (_, _, '001xx') => __UNALLOCATED
                        when (_, _, '0100x') => __UNALLOCATED
                        when (_, _, '01011') => __UNALLOCATED
                        when (_, _, '01101') => __UNALLOCATED
                        when (_, _, '01110') => __UNALLOCATED
                        when (_, _, '10xxx') => __UNALLOCATED
                        when (_, _, '1100x') => __UNALLOCATED
                        when (_, _, '111xx') => __UNALLOCATED
                        when ('0', _, '00011') => __encoding aarch64_vector_reduce_add_long // SADDLV_asimdall_only
                        when ('0', _, '01010') => __encoding aarch64_vector_reduce_int_max // SMAXV_asimdall_only
                        when ('0', _, '11010') => __encoding aarch64_vector_reduce_int_max // SMINV_asimdall_only
                        when ('0', _, '11011') => __encoding aarch64_vector_reduce_add_simd // ADDV_asimdall_only
                        when ('0', '00', '01100') => __encoding aarch64_vector_reduce_fp16_maxnm_simd // FMAXNMV_asimdall_only_H
                        when ('0', '00', '01111') => __encoding aarch64_vector_reduce_fp16_max_simd // FMAXV_asimdall_only_H
                        when ('0', '01', '01100') => __UNALLOCATED
                        when ('0', '01', '01111') => __UNALLOCATED
                        when ('0', '10', '01100') => __encoding aarch64_vector_reduce_fp16_maxnm_simd // FMINNMV_asimdall_only_H
                        when ('0', '10', '01111') => __encoding aarch64_vector_reduce_fp16_max_simd // FMINV_asimdall_only_H
                        when ('0', '11', '01100') => __UNALLOCATED
                        when ('0', '11', '01111') => __UNALLOCATED
                        when ('1', _, '00011') => __encoding aarch64_vector_reduce_add_long // UADDLV_asimdall_only
                        when ('1', _, '01010') => __encoding aarch64_vector_reduce_int_max // UMAXV_asimdall_only
                        when ('1', _, '11010') => __encoding aarch64_vector_reduce_int_max // UMINV_asimdall_only
                        when ('1', _, '11011') => __UNALLOCATED
                        when ('1', '0x', '01100') => __encoding aarch64_vector_reduce_fp_maxnm_simd // FMAXNMV_asimdall_only_SD
                        when ('1', '0x', '01111') => __encoding aarch64_vector_reduce_fp_max_simd // FMAXV_asimdall_only_SD
                        when ('1', '1x', '01100') => __encoding aarch64_vector_reduce_fp_maxnm_simd // FMINNMV_asimdall_only_SD
                        when ('1', '1x', '01111') => __encoding aarch64_vector_reduce_fp_max_simd // FMINV_asimdall_only_SD
                when ('0xx0', _, '0x', 'x1xx', '1xxxxxx10', _) => __UNPREDICTABLE
                when ('0xx0', _, '0x', 'x1xx', 'x1xxxxx10', _) => __UNPREDICTABLE
                when ('0xx0', _, '0x', 'x1xx', 'xxxxxxx00', _) => // asimddiff
                    __field Q 30 +: 1
                    __field U 29 +: 1
                    __field size 22 +: 2
                    __field Rm 16 +: 5
                    __field opcode 12 +: 4
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, opcode) of
                        when (_, '1111') => __UNALLOCATED
                        when ('0', '0000') => __encoding aarch64_vector_arithmetic_binary_disparate_add_sub_long // SADDL_asimddiff_L
                        when ('0', '0001') => __encoding aarch64_vector_arithmetic_binary_disparate_add_sub_wide // SADDW_asimddiff_W
                        when ('0', '0010') => __encoding aarch64_vector_arithmetic_binary_disparate_add_sub_long // SSUBL_asimddiff_L
                        when ('0', '0011') => __encoding aarch64_vector_arithmetic_binary_disparate_add_sub_wide // SSUBW_asimddiff_W
                        when ('0', '0100') => __encoding aarch64_vector_arithmetic_binary_disparate_add_sub_narrow // ADDHN_asimddiff_N
                        when ('0', '0101') => __encoding aarch64_vector_arithmetic_binary_disparate_diff // SABAL_asimddiff_L
                        when ('0', '0110') => __encoding aarch64_vector_arithmetic_binary_disparate_add_sub_narrow // SUBHN_asimddiff_N
                        when ('0', '0111') => __encoding aarch64_vector_arithmetic_binary_disparate_diff // SABDL_asimddiff_L
                        when ('0', '1000') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_accum // SMLAL_asimddiff_L
                        when ('0', '1001') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_dmacc_simd // SQDMLAL_asimddiff_L
                        when ('0', '1010') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_accum // SMLSL_asimddiff_L
                        when ('0', '1011') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_dmacc_simd // SQDMLSL_asimddiff_L
                        when ('0', '1100') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_product // SMULL_asimddiff_L
                        when ('0', '1101') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_double_simd // SQDMULL_asimddiff_L
                        when ('0', '1110') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_poly // PMULL_asimddiff_L
                        when ('1', '0000') => __encoding aarch64_vector_arithmetic_binary_disparate_add_sub_long // UADDL_asimddiff_L
                        when ('1', '0001') => __encoding aarch64_vector_arithmetic_binary_disparate_add_sub_wide // UADDW_asimddiff_W
                        when ('1', '0010') => __encoding aarch64_vector_arithmetic_binary_disparate_add_sub_long // USUBL_asimddiff_L
                        when ('1', '0011') => __encoding aarch64_vector_arithmetic_binary_disparate_add_sub_wide // USUBW_asimddiff_W
                        when ('1', '0100') => __encoding aarch64_vector_arithmetic_binary_disparate_add_sub_narrow // RADDHN_asimddiff_N
                        when ('1', '0101') => __encoding aarch64_vector_arithmetic_binary_disparate_diff // UABAL_asimddiff_L
                        when ('1', '0110') => __encoding aarch64_vector_arithmetic_binary_disparate_add_sub_narrow // RSUBHN_asimddiff_N
                        when ('1', '0111') => __encoding aarch64_vector_arithmetic_binary_disparate_diff // UABDL_asimddiff_L
                        when ('1', '1000') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_accum // UMLAL_asimddiff_L
                        when ('1', '1001') => __UNALLOCATED
                        when ('1', '1010') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_accum // UMLSL_asimddiff_L
                        when ('1', '1011') => __UNALLOCATED
                        when ('1', '1100') => __encoding aarch64_vector_arithmetic_binary_disparate_mul_product // UMULL_asimddiff_L
                        when ('1', '1101') => __UNALLOCATED
                        when ('1', '1110') => __UNALLOCATED
                when ('0xx0', _, '0x', 'x1xx', 'xxxxxxxx1', _) => // asimdsame
                    __field Q 30 +: 1
                    __field U 29 +: 1
                    __field size 22 +: 2
                    __field Rm 16 +: 5
                    __field opcode 11 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, size, opcode) of
                        when ('0', _, '00000') => __encoding aarch64_vector_arithmetic_binary_uniform_add_halving_truncating // SHADD_asimdsame_only
                        when ('0', _, '00001') => __encoding aarch64_vector_arithmetic_binary_uniform_add_saturating_simd // SQADD_asimdsame_only
                        when ('0', _, '00010') => __encoding aarch64_vector_arithmetic_binary_uniform_add_halving_rounding // SRHADD_asimdsame_only
                        when ('0', _, '00100') => __encoding aarch64_vector_arithmetic_binary_uniform_sub_int // SHSUB_asimdsame_only
                        when ('0', _, '00101') => __encoding aarch64_vector_arithmetic_binary_uniform_sub_saturating_simd // SQSUB_asimdsame_only
                        when ('0', _, '00110') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_int_simd // CMGT_asimdsame_only
                        when ('0', _, '00111') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_int_simd // CMGE_asimdsame_only
                        when ('0', _, '01000') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_simd // SSHL_asimdsame_only
                        when ('0', _, '01001') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_simd // SQSHL_asimdsame_only
                        when ('0', _, '01010') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_simd // SRSHL_asimdsame_only
                        when ('0', _, '01011') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_simd // SQRSHL_asimdsame_only
                        when ('0', _, '01100') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_single // SMAX_asimdsame_only
                        when ('0', _, '01101') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_single // SMIN_asimdsame_only
                        when ('0', _, '01110') => __encoding aarch64_vector_arithmetic_binary_uniform_diff // SABD_asimdsame_only
                        when ('0', _, '01111') => __encoding aarch64_vector_arithmetic_binary_uniform_diff // SABA_asimdsame_only
                        when ('0', _, '10000') => __encoding aarch64_vector_arithmetic_binary_uniform_add_wrapping_single_simd // ADD_asimdsame_only
                        when ('0', _, '10001') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_bitwise_simd // CMTST_asimdsame_only
                        when ('0', _, '10010') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_accum // MLA_asimdsame_only
                        when ('0', _, '10011') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_product // MUL_asimdsame_only
                        when ('0', _, '10100') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_pair // SMAXP_asimdsame_only
                        when ('0', _, '10101') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_pair // SMINP_asimdsame_only
                        when ('0', _, '10110') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_doubling_simd // SQDMULH_asimdsame_only
                        when ('0', _, '10111') => __encoding aarch64_vector_arithmetic_binary_uniform_add_wrapping_pair // ADDP_asimdsame_only
                        when ('0', '0x', '11000') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp_2008 // FMAXNM_asimdsame_only
                        when ('0', '0x', '11001') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp_fused // FMLA_asimdsame_only
                        when ('0', '0x', '11010') => __encoding aarch64_vector_arithmetic_binary_uniform_add_fp // FADD_asimdsame_only
                        when ('0', '0x', '11011') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp_extended_simd // FMULX_asimdsame_only
                        when ('0', '0x', '11100') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp_simd // FCMEQ_asimdsame_only
                        when ('0', '0x', '11110') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp_1985 // FMAX_asimdsame_only
                        when ('0', '0x', '11111') => __encoding aarch64_vector_arithmetic_binary_uniform_recps_simd // FRECPS_asimdsame_only
                        when ('0', '00', '00011') => __encoding aarch64_vector_arithmetic_binary_uniform_logical_and_orr // AND_asimdsame_only
                        when ('0', '00', '11101') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp_mul_norounding_lower // FMLAL_asimdsame_F
                        when ('0', '01', '00011') => __encoding aarch64_vector_arithmetic_binary_uniform_logical_and_orr // BIC_asimdsame_only
                        when ('0', '01', '11101') => __UNALLOCATED
                        when ('0', '1x', '11000') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp_2008 // FMINNM_asimdsame_only
                        when ('0', '1x', '11001') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp_fused // FMLS_asimdsame_only
                        when ('0', '1x', '11010') => __encoding aarch64_vector_arithmetic_binary_uniform_sub_fp_simd // FSUB_asimdsame_only
                        when ('0', '1x', '11011') => __UNALLOCATED
                        when ('0', '1x', '11100') => __UNALLOCATED
                        when ('0', '1x', '11110') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp_1985 // FMIN_asimdsame_only
                        when ('0', '1x', '11111') => __encoding aarch64_vector_arithmetic_binary_uniform_rsqrts_simd // FRSQRTS_asimdsame_only
                        when ('0', '10', '00011') => __encoding aarch64_vector_arithmetic_binary_uniform_logical_and_orr // ORR_asimdsame_only
                        when ('0', '10', '11101') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp_mul_norounding_lower // FMLSL_asimdsame_F
                        when ('0', '11', '00011') => __encoding aarch64_vector_arithmetic_binary_uniform_logical_and_orr // ORN_asimdsame_only
                        when ('0', '11', '11101') => __UNALLOCATED
                        when ('1', _, '00000') => __encoding aarch64_vector_arithmetic_binary_uniform_add_halving_truncating // UHADD_asimdsame_only
                        when ('1', _, '00001') => __encoding aarch64_vector_arithmetic_binary_uniform_add_saturating_simd // UQADD_asimdsame_only
                        when ('1', _, '00010') => __encoding aarch64_vector_arithmetic_binary_uniform_add_halving_rounding // URHADD_asimdsame_only
                        when ('1', _, '00100') => __encoding aarch64_vector_arithmetic_binary_uniform_sub_int // UHSUB_asimdsame_only
                        when ('1', _, '00101') => __encoding aarch64_vector_arithmetic_binary_uniform_sub_saturating_simd // UQSUB_asimdsame_only
                        when ('1', _, '00110') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_int_simd // CMHI_asimdsame_only
                        when ('1', _, '00111') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_int_simd // CMHS_asimdsame_only
                        when ('1', _, '01000') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_simd // USHL_asimdsame_only
                        when ('1', _, '01001') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_simd // UQSHL_asimdsame_only
                        when ('1', _, '01010') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_simd // URSHL_asimdsame_only
                        when ('1', _, '01011') => __encoding aarch64_vector_arithmetic_binary_uniform_shift_simd // UQRSHL_asimdsame_only
                        when ('1', _, '01100') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_single // UMAX_asimdsame_only
                        when ('1', _, '01101') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_single // UMIN_asimdsame_only
                        when ('1', _, '01110') => __encoding aarch64_vector_arithmetic_binary_uniform_diff // UABD_asimdsame_only
                        when ('1', _, '01111') => __encoding aarch64_vector_arithmetic_binary_uniform_diff // UABA_asimdsame_only
                        when ('1', _, '10000') => __encoding aarch64_vector_arithmetic_binary_uniform_add_wrapping_single_simd // SUB_asimdsame_only
                        when ('1', _, '10001') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_bitwise_simd // CMEQ_asimdsame_only
                        when ('1', _, '10010') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_accum // MLS_asimdsame_only
                        when ('1', _, '10011') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_product // PMUL_asimdsame_only
                        when ('1', _, '10100') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_pair // UMAXP_asimdsame_only
                        when ('1', _, '10101') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_pair // UMINP_asimdsame_only
                        when ('1', _, '10110') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_int_doubling_simd // SQRDMULH_asimdsame_only
                        when ('1', _, '10111') => __UNALLOCATED
                        when ('1', '0x', '11000') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp_2008 // FMAXNMP_asimdsame_only
                        when ('1', '0x', '11010') => __encoding aarch64_vector_arithmetic_binary_uniform_add_fp // FADDP_asimdsame_only
                        when ('1', '0x', '11011') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp_product // FMUL_asimdsame_only
                        when ('1', '0x', '11100') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp_simd // FCMGE_asimdsame_only
                        when ('1', '0x', '11101') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp_simd // FACGE_asimdsame_only
                        when ('1', '0x', '11110') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp_1985 // FMAXP_asimdsame_only
                        when ('1', '0x', '11111') => __encoding aarch64_vector_arithmetic_binary_uniform_div // FDIV_asimdsame_only
                        when ('1', '00', '00011') => __encoding aarch64_vector_arithmetic_binary_uniform_logical_bsl_eor // EOR_asimdsame_only
                        when ('1', '00', '11001') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp_mul_norounding_upper // FMLAL2_asimdsame_F
                        when ('1', '01', '00011') => __encoding aarch64_vector_arithmetic_binary_uniform_logical_bsl_eor // BSL_asimdsame_only
                        when ('1', '01', '11001') => __UNALLOCATED
                        when ('1', '1x', '11000') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp_2008 // FMINNMP_asimdsame_only
                        when ('1', '1x', '11010') => __encoding aarch64_vector_arithmetic_binary_uniform_sub_fp_simd // FABD_asimdsame_only
                        when ('1', '1x', '11011') => __UNALLOCATED
                        when ('1', '1x', '11100') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp_simd // FCMGT_asimdsame_only
                        when ('1', '1x', '11101') => __encoding aarch64_vector_arithmetic_binary_uniform_cmp_fp_simd // FACGT_asimdsame_only
                        when ('1', '1x', '11110') => __encoding aarch64_vector_arithmetic_binary_uniform_max_min_fp_1985 // FMINP_asimdsame_only
                        when ('1', '1x', '11111') => __UNALLOCATED
                        when ('1', '10', '00011') => __encoding aarch64_vector_arithmetic_binary_uniform_logical_bsl_eor // BIT_asimdsame_only
                        when ('1', '10', '11001') => __encoding aarch64_vector_arithmetic_binary_uniform_mul_fp_mul_norounding_upper // FMLSL2_asimdsame_F
                        when ('1', '11', '00011') => __encoding aarch64_vector_arithmetic_binary_uniform_logical_bsl_eor // BIF_asimdsame_only
                        when ('1', '11', '11001') => __UNALLOCATED
                when ('0xx0', _, '10', '0000', 'xxxxxxxx1', _) => // asimdimm
                    __field Q 30 +: 1
                    __field op 29 +: 1
                    __field a 18 +: 1
                    __field b 17 +: 1
                    __field c 16 +: 1
                    __field cmode 12 +: 4
                    __field o2 11 +: 1
                    __field d 9 +: 1
                    __field e 8 +: 1
                    __field f 7 +: 1
                    __field g 6 +: 1
                    __field h 5 +: 1
                    __field Rd 0 +: 5
                    case (Q, op, cmode, o2) of
                        when (_, '0', '0xxx', '1') => __UNALLOCATED
                        when (_, '0', '0xx0', '0') => __encoding aarch64_vector_logical // MOVI_asimdimm_L_sl
                        when (_, '0', '0xx1', '0') => __encoding aarch64_vector_logical // ORR_asimdimm_L_sl
                        when (_, '0', '10xx', '1') => __UNALLOCATED
                        when (_, '0', '10x0', '0') => __encoding aarch64_vector_logical // MOVI_asimdimm_L_hl
                        when (_, '0', '10x1', '0') => __encoding aarch64_vector_logical // ORR_asimdimm_L_hl
                        when (_, '0', '110x', '0') => __encoding aarch64_vector_logical // MOVI_asimdimm_M_sm
                        when (_, '0', '110x', '1') => __UNALLOCATED
                        when (_, '0', '1110', '0') => __encoding aarch64_vector_logical // MOVI_asimdimm_N_b
                        when (_, '0', '1110', '1') => __UNALLOCATED
                        when (_, '0', '1111', '0') => __encoding aarch64_vector_logical // FMOV_asimdimm_S_s
                        when (_, '0', '1111', '1') => __encoding aarch64_vector_fp16_movi // FMOV_asimdimm_H_h
                        when (_, '1', _, '1') => __UNALLOCATED
                        when (_, '1', '0xx0', '0') => __encoding aarch64_vector_logical // MVNI_asimdimm_L_sl
                        when (_, '1', '0xx1', '0') => __encoding aarch64_vector_logical // BIC_asimdimm_L_sl
                        when (_, '1', '10x0', '0') => __encoding aarch64_vector_logical // MVNI_asimdimm_L_hl
                        when (_, '1', '10x1', '0') => __encoding aarch64_vector_logical // BIC_asimdimm_L_hl
                        when (_, '1', '110x', '0') => __encoding aarch64_vector_logical // MVNI_asimdimm_M_sm
                        when ('0', '1', '1110', '0') => __encoding aarch64_vector_logical // MOVI_asimdimm_D_ds
                        when ('0', '1', '1111', '0') => __UNALLOCATED
                        when ('1', '1', '1110', '0') => __encoding aarch64_vector_logical // MOVI_asimdimm_D2_d
                        when ('1', '1', '1111', '0') => __encoding aarch64_vector_logical // FMOV_asimdimm_D2_d
                when ('0xx0', _, '10', !'0000', 'xxxxxxxx1', _) => // asimdshf
                    __field Q 30 +: 1
                    __field U 29 +: 1
                    __field immh 19 +: 4
                    __field immb 16 +: 3
                    __field opcode 11 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, opcode) of
                        when (_, '00001') => __UNALLOCATED
                        when (_, '00011') => __UNALLOCATED
                        when (_, '00101') => __UNALLOCATED
                        when (_, '00111') => __UNALLOCATED
                        when (_, '01001') => __UNALLOCATED
                        when (_, '01011') => __UNALLOCATED
                        when (_, '01101') => __UNALLOCATED
                        when (_, '01111') => __UNALLOCATED
                        when (_, '10101') => __UNALLOCATED
                        when (_, '1011x') => __UNALLOCATED
                        when (_, '110xx') => __UNALLOCATED
                        when (_, '11101') => __UNALLOCATED
                        when (_, '11110') => __UNALLOCATED
                        when ('0', '00000') => __encoding aarch64_vector_shift_right_simd // SSHR_asimdshf_R
                        when ('0', '00010') => __encoding aarch64_vector_shift_right_simd // SSRA_asimdshf_R
                        when ('0', '00100') => __encoding aarch64_vector_shift_right_simd // SRSHR_asimdshf_R
                        when ('0', '00110') => __encoding aarch64_vector_shift_right_simd // SRSRA_asimdshf_R
                        when ('0', '01000') => __UNALLOCATED
                        when ('0', '01010') => __encoding aarch64_vector_shift_left_simd // SHL_asimdshf_R
                        when ('0', '01100') => __UNALLOCATED
                        when ('0', '01110') => __encoding aarch64_vector_shift_left_sat_simd // SQSHL_asimdshf_R
                        when ('0', '10000') => __encoding aarch64_vector_shift_right_narrow_logical // SHRN_asimdshf_N
                        when ('0', '10001') => __encoding aarch64_vector_shift_right_narrow_logical // RSHRN_asimdshf_N
                        when ('0', '10010') => __encoding aarch64_vector_shift_right_narrow_uniform_simd // SQSHRN_asimdshf_N
                        when ('0', '10011') => __encoding aarch64_vector_shift_right_narrow_uniform_simd // SQRSHRN_asimdshf_N
                        when ('0', '10100') => __encoding aarch64_vector_shift_left_long // SSHLL_asimdshf_L
                        when ('0', '11100') => __encoding aarch64_vector_shift_conv_int_simd // SCVTF_asimdshf_C
                        when ('0', '11111') => __encoding aarch64_vector_shift_conv_float_simd // FCVTZS_asimdshf_C
                        when ('1', '00000') => __encoding aarch64_vector_shift_right_simd // USHR_asimdshf_R
                        when ('1', '00010') => __encoding aarch64_vector_shift_right_simd // USRA_asimdshf_R
                        when ('1', '00100') => __encoding aarch64_vector_shift_right_simd // URSHR_asimdshf_R
                        when ('1', '00110') => __encoding aarch64_vector_shift_right_simd // URSRA_asimdshf_R
                        when ('1', '01000') => __encoding aarch64_vector_shift_right_insert_simd // SRI_asimdshf_R
                        when ('1', '01010') => __encoding aarch64_vector_shift_left_insert_simd // SLI_asimdshf_R
                        when ('1', '01100') => __encoding aarch64_vector_shift_left_sat_simd // SQSHLU_asimdshf_R
                        when ('1', '01110') => __encoding aarch64_vector_shift_left_sat_simd // UQSHL_asimdshf_R
                        when ('1', '10000') => __encoding aarch64_vector_shift_right_narrow_nonuniform_simd // SQSHRUN_asimdshf_N
                        when ('1', '10001') => __encoding aarch64_vector_shift_right_narrow_nonuniform_simd // SQRSHRUN_asimdshf_N
                        when ('1', '10010') => __encoding aarch64_vector_shift_right_narrow_uniform_simd // UQSHRN_asimdshf_N
                        when ('1', '10011') => __encoding aarch64_vector_shift_right_narrow_uniform_simd // UQRSHRN_asimdshf_N
                        when ('1', '10100') => __encoding aarch64_vector_shift_left_long // USHLL_asimdshf_L
                        when ('1', '11100') => __encoding aarch64_vector_shift_conv_int_simd // UCVTF_asimdshf_C
                        when ('1', '11111') => __encoding aarch64_vector_shift_conv_float_simd // FCVTZU_asimdshf_C
                when ('0xx0', _, '11', _, 'xxxxxxxx1', _) => __UNPREDICTABLE
                when ('0xx0', _, '1x', _, 'xxxxxxxx0', _) => // asimdelem
                    __field Q 30 +: 1
                    __field U 29 +: 1
                    __field size 22 +: 2
                    __field L 21 +: 1
                    __field M 20 +: 1
                    __field Rm 16 +: 4
                    __field opcode 12 +: 4
                    __field H 11 +: 1
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (U, size, opcode) of
                        when (_, '01', '1001') => __UNALLOCATED
                        when ('0', _, '0010') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_long // SMLAL_asimdelem_L
                        when ('0', _, '0011') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_double_simd // SQDMLAL_asimdelem_L
                        when ('0', _, '0110') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_long // SMLSL_asimdelem_L
                        when ('0', _, '0111') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_double_simd // SQDMLSL_asimdelem_L
                        when ('0', _, '1000') => __encoding aarch64_vector_arithmetic_binary_element_mul_int // MUL_asimdelem_R
                        when ('0', _, '1010') => __encoding aarch64_vector_arithmetic_binary_element_mul_long // SMULL_asimdelem_L
                        when ('0', _, '1011') => __encoding aarch64_vector_arithmetic_binary_element_mul_double_simd // SQDMULL_asimdelem_L
                        when ('0', _, '1100') => __encoding aarch64_vector_arithmetic_binary_element_mul_high_simd // SQDMULH_asimdelem_R
                        when ('0', _, '1101') => __encoding aarch64_vector_arithmetic_binary_element_mul_high_simd // SQRDMULH_asimdelem_R
                        when ('0', _, '1110') => __encoding aarch64_vector_arithmetic_binary_element_dotp // SDOT_asimdelem_D
                        when ('0', '0x', '0000') => __UNALLOCATED
                        when ('0', '0x', '0100') => __UNALLOCATED
                        when ('0', '00', '0001') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_fp16_simd // FMLA_asimdelem_RH_H
                        when ('0', '00', '0101') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_fp16_simd // FMLS_asimdelem_RH_H
                        when ('0', '00', '1001') => __encoding aarch64_vector_arithmetic_binary_element_mul_fp16_simd // FMUL_asimdelem_RH_H
                        when ('0', '00', '1111') => __UNALLOCATED
                        when ('0', '01', '0001') => __UNALLOCATED
                        when ('0', '01', '0101') => __UNALLOCATED
                        when ('0', '1x', '0001') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_fp_simd // FMLA_asimdelem_R_SD
                        when ('0', '1x', '0101') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_fp_simd // FMLS_asimdelem_R_SD
                        when ('0', '1x', '1001') => __encoding aarch64_vector_arithmetic_binary_element_mul_fp_simd // FMUL_asimdelem_R_SD
                        when ('0', '1x', '1111') => __UNALLOCATED
                        when ('0', '10', '0000') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_mul_norounding_i_lower // FMLAL_asimdelem_LH
                        when ('0', '10', '0100') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_mul_norounding_i_lower // FMLSL_asimdelem_LH
                        when ('0', '11', '0000') => __UNALLOCATED
                        when ('0', '11', '0100') => __UNALLOCATED
                        when ('1', _, '0000') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_int // MLA_asimdelem_R
                        when ('1', _, '0010') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_long // UMLAL_asimdelem_L
                        when ('1', _, '0100') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_int // MLS_asimdelem_R
                        when ('1', _, '0110') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_long // UMLSL_asimdelem_L
                        when ('1', _, '1010') => __encoding aarch64_vector_arithmetic_binary_element_mul_long // UMULL_asimdelem_L
                        when ('1', _, '1011') => __UNALLOCATED
                        when ('1', _, '1101') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_high_simd // SQRDMLAH_asimdelem_R
                        when ('1', _, '1110') => __encoding aarch64_vector_arithmetic_binary_element_dotp // UDOT_asimdelem_D
                        when ('1', _, '1111') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_high_simd // SQRDMLSH_asimdelem_R
                        when ('1', '0x', '1000') => __UNALLOCATED
                        when ('1', '0x', '1100') => __UNALLOCATED
                        when ('1', '00', '0001') => __UNALLOCATED
                        when ('1', '00', '0011') => __UNALLOCATED
                        when ('1', '00', '0101') => __UNALLOCATED
                        when ('1', '00', '0111') => __UNALLOCATED
                        when ('1', '00', '1001') => __encoding aarch64_vector_arithmetic_binary_element_mul_fp16_simd // FMULX_asimdelem_RH_H
                        when ('1', '01', '0xx1') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_complex // FCMLA_asimdelem_C_H
                        when ('1', '1x', '1001') => __encoding aarch64_vector_arithmetic_binary_element_mul_fp_simd // FMULX_asimdelem_R_SD
                        when ('1', '10', '0xx1') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_complex // FCMLA_asimdelem_C_S
                        when ('1', '10', '1000') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_mul_norounding_i_upper // FMLAL2_asimdelem_LH
                        when ('1', '10', '1100') => __encoding aarch64_vector_arithmetic_binary_element_mul_acc_mul_norounding_i_upper // FMLSL2_asimdelem_LH
                        when ('1', '11', '0001') => __UNALLOCATED
                        when ('1', '11', '0011') => __UNALLOCATED
                        when ('1', '11', '0101') => __UNALLOCATED
                        when ('1', '11', '0111') => __UNALLOCATED
                        when ('1', '11', '1000') => __UNALLOCATED
                        when ('1', '11', '1100') => __UNALLOCATED
                when ('1100', _, '00', '10xx', 'xxx10xxxx', _) => // crypto3_imm2
                    __field Rm 16 +: 5
                    __field imm2 12 +: 2
                    __field opcode 10 +: 2
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (opcode) of
                        when ('00') => __encoding aarch64_vector_crypto_sm3_sm3tt1a // SM3TT1A_VVV4_crypto3_imm2
                        when ('01') => __encoding aarch64_vector_crypto_sm3_sm3tt1b // SM3TT1B_VVV4_crypto3_imm2
                        when ('10') => __encoding aarch64_vector_crypto_sm3_sm3tt2a // SM3TT2A_VVV4_crypto3_imm2
                        when ('11') => __encoding aarch64_vector_crypto_sm3_sm3tt2b // SM3TT2B_VVV_crypto3_imm2
                when ('1100', _, '00', '11xx', 'xxx1x00xx', _) => // cryptosha512_3
                    __field Rm 16 +: 5
                    __field O 14 +: 1
                    __field opcode 10 +: 2
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (O, opcode) of
                        when ('0', '00') => __encoding aarch64_vector_crypto_sha512_sha512h // SHA512H_QQV_cryptosha512_3
                        when ('0', '01') => __encoding aarch64_vector_crypto_sha512_sha512h2 // SHA512H2_QQV_cryptosha512_3
                        when ('0', '10') => __encoding aarch64_vector_crypto_sha512_sha512su1 // SHA512SU1_VVV2_cryptosha512_3
                        when ('0', '11') => __encoding aarch64_vector_crypto_sha3_rax1 // RAX1_VVV2_cryptosha512_3
                        when ('1', '00') => __encoding aarch64_vector_crypto_sm3_sm3partw1 // SM3PARTW1_VVV4_cryptosha512_3
                        when ('1', '01') => __encoding aarch64_vector_crypto_sm3_sm3partw2 // SM3PARTW2_VVV4_cryptosha512_3
                        when ('1', '10') => __encoding aarch64_vector_crypto_sm4_sm4enckey // SM4EKEY_VVV4_cryptosha512_3
                        when ('1', '11') => __UNALLOCATED
                when ('1100', _, '00', _, 'xxx0xxxxx', _) => // crypto4
                    __field Op0 21 +: 2
                    __field Rm 16 +: 5
                    __field Ra 10 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (Op0) of
                        when ('00') => __encoding aarch64_vector_crypto_sha3_eor3 // EOR3_VVV16_crypto4
                        when ('01') => __encoding aarch64_vector_crypto_sha3_bcax // BCAX_VVV16_crypto4
                        when ('10') => __encoding aarch64_vector_crypto_sm3_sm3ss1 // SM3SS1_VVV4_crypto4
                        when ('11') => __UNALLOCATED
                when ('1100', _, '01', '00xx', _, _) => // crypto3_imm6
                    __field Rm 16 +: 5
                    __field imm6 10 +: 6
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case () of
                        when () => __encoding aarch64_vector_crypto_sha3_xar // XAR_VVV2_crypto3_imm6
                when ('1100', _, '01', '1000', '0001000xx', _) => // cryptosha512_2
                    __field opcode 10 +: 2
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (opcode) of
                        when ('00') => __encoding aarch64_vector_crypto_sha512_sha512su0 // SHA512SU0_VV2_cryptosha512_2
                        when ('01') => __encoding aarch64_vector_crypto_sm4_sm4enc // SM4E_VV4_cryptosha512_2
                        when ('1x') => __UNALLOCATED
                when ('1xx0', _, '1x', _, _, _) => __UNPREDICTABLE
                when ('x0x1', _, '0x', 'x0xx', _, _) => // float2fix
                    __field sf 31 +: 1
                    __field S 29 +: 1
                    __field type1 22 +: 2
                    __field rmode 19 +: 2
                    __field opcode 16 +: 3
                    __field scale 10 +: 6
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, S, type1, rmode, opcode, scale) of
                        when (_, _, _, _, '1xx', _) => __UNALLOCATED
                        when (_, _, _, 'x0', '00x', _) => __UNALLOCATED
                        when (_, _, _, 'x1', '01x', _) => __UNALLOCATED
                        when (_, _, _, '0x', '00x', _) => __UNALLOCATED
                        when (_, _, _, '1x', '01x', _) => __UNALLOCATED
                        when (_, _, '10', _, _, _) => __UNALLOCATED
                        when (_, '1', _, _, _, _) => __UNALLOCATED
                        when ('0', _, _, _, _, '0xxxxx') => __UNALLOCATED
                        when ('0', '0', '00', '00', '010', _) => __encoding aarch64_float_convert_fix // SCVTF_S32_float2fix
                        when ('0', '0', '00', '00', '011', _) => __encoding aarch64_float_convert_fix // UCVTF_S32_float2fix
                        when ('0', '0', '00', '11', '000', _) => __encoding aarch64_float_convert_fix // FCVTZS_32S_float2fix
                        when ('0', '0', '00', '11', '001', _) => __encoding aarch64_float_convert_fix // FCVTZU_32S_float2fix
                        when ('0', '0', '01', '00', '010', _) => __encoding aarch64_float_convert_fix // SCVTF_D32_float2fix
                        when ('0', '0', '01', '00', '011', _) => __encoding aarch64_float_convert_fix // UCVTF_D32_float2fix
                        when ('0', '0', '01', '11', '000', _) => __encoding aarch64_float_convert_fix // FCVTZS_32D_float2fix
                        when ('0', '0', '01', '11', '001', _) => __encoding aarch64_float_convert_fix // FCVTZU_32D_float2fix
                        when ('0', '0', '11', '00', '010', _) => __encoding aarch64_float_convert_fix // SCVTF_H32_float2fix
                        when ('0', '0', '11', '00', '011', _) => __encoding aarch64_float_convert_fix // UCVTF_H32_float2fix
                        when ('0', '0', '11', '11', '000', _) => __encoding aarch64_float_convert_fix // FCVTZS_32H_float2fix
                        when ('0', '0', '11', '11', '001', _) => __encoding aarch64_float_convert_fix // FCVTZU_32H_float2fix
                        when ('1', '0', '00', '00', '010', _) => __encoding aarch64_float_convert_fix // SCVTF_S64_float2fix
                        when ('1', '0', '00', '00', '011', _) => __encoding aarch64_float_convert_fix // UCVTF_S64_float2fix
                        when ('1', '0', '00', '11', '000', _) => __encoding aarch64_float_convert_fix // FCVTZS_64S_float2fix
                        when ('1', '0', '00', '11', '001', _) => __encoding aarch64_float_convert_fix // FCVTZU_64S_float2fix
                        when ('1', '0', '01', '00', '010', _) => __encoding aarch64_float_convert_fix // SCVTF_D64_float2fix
                        when ('1', '0', '01', '00', '011', _) => __encoding aarch64_float_convert_fix // UCVTF_D64_float2fix
                        when ('1', '0', '01', '11', '000', _) => __encoding aarch64_float_convert_fix // FCVTZS_64D_float2fix
                        when ('1', '0', '01', '11', '001', _) => __encoding aarch64_float_convert_fix // FCVTZU_64D_float2fix
                        when ('1', '0', '11', '00', '010', _) => __encoding aarch64_float_convert_fix // SCVTF_H64_float2fix
                        when ('1', '0', '11', '00', '011', _) => __encoding aarch64_float_convert_fix // UCVTF_H64_float2fix
                        when ('1', '0', '11', '11', '000', _) => __encoding aarch64_float_convert_fix // FCVTZS_64H_float2fix
                        when ('1', '0', '11', '11', '001', _) => __encoding aarch64_float_convert_fix // FCVTZU_64H_float2fix
                when ('x0x1', _, '0x', 'x1xx', 'xxx000000', _) => // float2int
                    __field sf 31 +: 1
                    __field S 29 +: 1
                    __field type1 22 +: 2
                    __field rmode 19 +: 2
                    __field opcode 16 +: 3
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (sf, S, type1, rmode, opcode) of
                        when (_, _, _, 'x1', '01x') => __UNALLOCATED
                        when (_, _, _, 'x1', '10x') => __UNALLOCATED
                        when (_, _, _, '1x', '01x') => __UNALLOCATED
                        when (_, _, _, '1x', '10x') => __UNALLOCATED
                        when (_, '0', '10', _, '0xx') => __UNALLOCATED
                        when (_, '0', '10', _, '10x') => __UNALLOCATED
                        when (_, '1', _, _, _) => __UNALLOCATED
                        when ('0', '0', '00', 'x1', '11x') => __UNALLOCATED
                        when ('0', '0', '00', '00', '000') => __encoding aarch64_float_convert_int // FCVTNS_32S_float2int
                        when ('0', '0', '00', '00', '001') => __encoding aarch64_float_convert_int // FCVTNU_32S_float2int
                        when ('0', '0', '00', '00', '010') => __encoding aarch64_float_convert_int // SCVTF_S32_float2int
                        when ('0', '0', '00', '00', '011') => __encoding aarch64_float_convert_int // UCVTF_S32_float2int
                        when ('0', '0', '00', '00', '100') => __encoding aarch64_float_convert_int // FCVTAS_32S_float2int
                        when ('0', '0', '00', '00', '101') => __encoding aarch64_float_convert_int // FCVTAU_32S_float2int
                        when ('0', '0', '00', '00', '110') => __encoding aarch64_float_convert_int // FMOV_32S_float2int
                        when ('0', '0', '00', '00', '111') => __encoding aarch64_float_convert_int // FMOV_S32_float2int
                        when ('0', '0', '00', '01', '000') => __encoding aarch64_float_convert_int // FCVTPS_32S_float2int
                        when ('0', '0', '00', '01', '001') => __encoding aarch64_float_convert_int // FCVTPU_32S_float2int
                        when ('0', '0', '00', '1x', '11x') => __UNALLOCATED
                        when ('0', '0', '00', '10', '000') => __encoding aarch64_float_convert_int // FCVTMS_32S_float2int
                        when ('0', '0', '00', '10', '001') => __encoding aarch64_float_convert_int // FCVTMU_32S_float2int
                        when ('0', '0', '00', '11', '000') => __encoding aarch64_float_convert_int // FCVTZS_32S_float2int
                        when ('0', '0', '00', '11', '001') => __encoding aarch64_float_convert_int // FCVTZU_32S_float2int
                        when ('0', '0', '01', '0x', '11x') => __UNALLOCATED
                        when ('0', '0', '01', '00', '000') => __encoding aarch64_float_convert_int // FCVTNS_32D_float2int
                        when ('0', '0', '01', '00', '001') => __encoding aarch64_float_convert_int // FCVTNU_32D_float2int
                        when ('0', '0', '01', '00', '010') => __encoding aarch64_float_convert_int // SCVTF_D32_float2int
                        when ('0', '0', '01', '00', '011') => __encoding aarch64_float_convert_int // UCVTF_D32_float2int
                        when ('0', '0', '01', '00', '100') => __encoding aarch64_float_convert_int // FCVTAS_32D_float2int
                        when ('0', '0', '01', '00', '101') => __encoding aarch64_float_convert_int // FCVTAU_32D_float2int
                        when ('0', '0', '01', '01', '000') => __encoding aarch64_float_convert_int // FCVTPS_32D_float2int
                        when ('0', '0', '01', '01', '001') => __encoding aarch64_float_convert_int // FCVTPU_32D_float2int
                        when ('0', '0', '01', '10', '000') => __encoding aarch64_float_convert_int // FCVTMS_32D_float2int
                        when ('0', '0', '01', '10', '001') => __encoding aarch64_float_convert_int // FCVTMU_32D_float2int
                        when ('0', '0', '01', '10', '11x') => __UNALLOCATED
                        when ('0', '0', '01', '11', '000') => __encoding aarch64_float_convert_int // FCVTZS_32D_float2int
                        when ('0', '0', '01', '11', '001') => __encoding aarch64_float_convert_int // FCVTZU_32D_float2int
                        when ('0', '0', '01', '11', '110') => __encoding aarch64_float_convert_int // FJCVTZS_32D_float2int
                        when ('0', '0', '01', '11', '111') => __UNALLOCATED
                        when ('0', '0', '10', _, '11x') => __UNALLOCATED
                        when ('0', '0', '11', '00', '000') => __encoding aarch64_float_convert_int // FCVTNS_32H_float2int
                        when ('0', '0', '11', '00', '001') => __encoding aarch64_float_convert_int // FCVTNU_32H_float2int
                        when ('0', '0', '11', '00', '010') => __encoding aarch64_float_convert_int // SCVTF_H32_float2int
                        when ('0', '0', '11', '00', '011') => __encoding aarch64_float_convert_int // UCVTF_H32_float2int
                        when ('0', '0', '11', '00', '100') => __encoding aarch64_float_convert_int // FCVTAS_32H_float2int
                        when ('0', '0', '11', '00', '101') => __encoding aarch64_float_convert_int // FCVTAU_32H_float2int
                        when ('0', '0', '11', '00', '110') => __encoding aarch64_float_convert_int // FMOV_32H_float2int
                        when ('0', '0', '11', '00', '111') => __encoding aarch64_float_convert_int // FMOV_H32_float2int
                        when ('0', '0', '11', '01', '000') => __encoding aarch64_float_convert_int // FCVTPS_32H_float2int
                        when ('0', '0', '11', '01', '001') => __encoding aarch64_float_convert_int // FCVTPU_32H_float2int
                        when ('0', '0', '11', '10', '000') => __encoding aarch64_float_convert_int // FCVTMS_32H_float2int
                        when ('0', '0', '11', '10', '001') => __encoding aarch64_float_convert_int // FCVTMU_32H_float2int
                        when ('0', '0', '11', '11', '000') => __encoding aarch64_float_convert_int // FCVTZS_32H_float2int
                        when ('0', '0', '11', '11', '001') => __encoding aarch64_float_convert_int // FCVTZU_32H_float2int
                        when ('1', '0', '00', _, '11x') => __UNALLOCATED
                        when ('1', '0', '00', '00', '000') => __encoding aarch64_float_convert_int // FCVTNS_64S_float2int
                        when ('1', '0', '00', '00', '001') => __encoding aarch64_float_convert_int // FCVTNU_64S_float2int
                        when ('1', '0', '00', '00', '010') => __encoding aarch64_float_convert_int // SCVTF_S64_float2int
                        when ('1', '0', '00', '00', '011') => __encoding aarch64_float_convert_int // UCVTF_S64_float2int
                        when ('1', '0', '00', '00', '100') => __encoding aarch64_float_convert_int // FCVTAS_64S_float2int
                        when ('1', '0', '00', '00', '101') => __encoding aarch64_float_convert_int // FCVTAU_64S_float2int
                        when ('1', '0', '00', '01', '000') => __encoding aarch64_float_convert_int // FCVTPS_64S_float2int
                        when ('1', '0', '00', '01', '001') => __encoding aarch64_float_convert_int // FCVTPU_64S_float2int
                        when ('1', '0', '00', '10', '000') => __encoding aarch64_float_convert_int // FCVTMS_64S_float2int
                        when ('1', '0', '00', '10', '001') => __encoding aarch64_float_convert_int // FCVTMU_64S_float2int
                        when ('1', '0', '00', '11', '000') => __encoding aarch64_float_convert_int // FCVTZS_64S_float2int
                        when ('1', '0', '00', '11', '001') => __encoding aarch64_float_convert_int // FCVTZU_64S_float2int
                        when ('1', '0', '01', 'x1', '11x') => __UNALLOCATED
                        when ('1', '0', '01', '00', '000') => __encoding aarch64_float_convert_int // FCVTNS_64D_float2int
                        when ('1', '0', '01', '00', '001') => __encoding aarch64_float_convert_int // FCVTNU_64D_float2int
                        when ('1', '0', '01', '00', '010') => __encoding aarch64_float_convert_int // SCVTF_D64_float2int
                        when ('1', '0', '01', '00', '011') => __encoding aarch64_float_convert_int // UCVTF_D64_float2int
                        when ('1', '0', '01', '00', '100') => __encoding aarch64_float_convert_int // FCVTAS_64D_float2int
                        when ('1', '0', '01', '00', '101') => __encoding aarch64_float_convert_int // FCVTAU_64D_float2int
                        when ('1', '0', '01', '00', '110') => __encoding aarch64_float_convert_int // FMOV_64D_float2int
                        when ('1', '0', '01', '00', '111') => __encoding aarch64_float_convert_int // FMOV_D64_float2int
                        when ('1', '0', '01', '01', '000') => __encoding aarch64_float_convert_int // FCVTPS_64D_float2int
                        when ('1', '0', '01', '01', '001') => __encoding aarch64_float_convert_int // FCVTPU_64D_float2int
                        when ('1', '0', '01', '1x', '11x') => __UNALLOCATED
                        when ('1', '0', '01', '10', '000') => __encoding aarch64_float_convert_int // FCVTMS_64D_float2int
                        when ('1', '0', '01', '10', '001') => __encoding aarch64_float_convert_int // FCVTMU_64D_float2int
                        when ('1', '0', '01', '11', '000') => __encoding aarch64_float_convert_int // FCVTZS_64D_float2int
                        when ('1', '0', '01', '11', '001') => __encoding aarch64_float_convert_int // FCVTZU_64D_float2int
                        when ('1', '0', '10', 'x0', '11x') => __UNALLOCATED
                        when ('1', '0', '10', '01', '110') => __encoding aarch64_float_convert_int // FMOV_64VX_float2int
                        when ('1', '0', '10', '01', '111') => __encoding aarch64_float_convert_int // FMOV_V64I_float2int
                        when ('1', '0', '10', '1x', '11x') => __UNALLOCATED
                        when ('1', '0', '11', '00', '000') => __encoding aarch64_float_convert_int // FCVTNS_64H_float2int
                        when ('1', '0', '11', '00', '001') => __encoding aarch64_float_convert_int // FCVTNU_64H_float2int
                        when ('1', '0', '11', '00', '010') => __encoding aarch64_float_convert_int // SCVTF_H64_float2int
                        when ('1', '0', '11', '00', '011') => __encoding aarch64_float_convert_int // UCVTF_H64_float2int
                        when ('1', '0', '11', '00', '100') => __encoding aarch64_float_convert_int // FCVTAS_64H_float2int
                        when ('1', '0', '11', '00', '101') => __encoding aarch64_float_convert_int // FCVTAU_64H_float2int
                        when ('1', '0', '11', '00', '110') => __encoding aarch64_float_convert_int // FMOV_64H_float2int
                        when ('1', '0', '11', '00', '111') => __encoding aarch64_float_convert_int // FMOV_H64_float2int
                        when ('1', '0', '11', '01', '000') => __encoding aarch64_float_convert_int // FCVTPS_64H_float2int
                        when ('1', '0', '11', '01', '001') => __encoding aarch64_float_convert_int // FCVTPU_64H_float2int
                        when ('1', '0', '11', '10', '000') => __encoding aarch64_float_convert_int // FCVTMS_64H_float2int
                        when ('1', '0', '11', '10', '001') => __encoding aarch64_float_convert_int // FCVTMU_64H_float2int
                        when ('1', '0', '11', '11', '000') => __encoding aarch64_float_convert_int // FCVTZS_64H_float2int
                        when ('1', '0', '11', '11', '001') => __encoding aarch64_float_convert_int // FCVTZU_64H_float2int
                when ('x0x1', _, '0x', 'x1xx', 'xxxx10000', _) => // floatdp1
                    __field M 31 +: 1
                    __field S 29 +: 1
                    __field type1 22 +: 2
                    __field opcode 15 +: 6
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (M, S, type1, opcode) of
                        when (_, _, _, '1xxxxx') => __UNALLOCATED
                        when (_, '1', _, _) => __UNALLOCATED
                        when ('0', '0', '00', '000000') => __encoding aarch64_float_arithmetic_unary // FMOV_S_floatdp1
                        when ('0', '0', '00', '000001') => __encoding aarch64_float_arithmetic_unary // FABS_S_floatdp1
                        when ('0', '0', '00', '000010') => __encoding aarch64_float_arithmetic_unary // FNEG_S_floatdp1
                        when ('0', '0', '00', '000011') => __encoding aarch64_float_arithmetic_unary // FSQRT_S_floatdp1
                        when ('0', '0', '00', '000100') => __UNALLOCATED
                        when ('0', '0', '00', '000101') => __encoding aarch64_float_convert_fp // FCVT_DS_floatdp1
                        when ('0', '0', '00', '000110') => __UNALLOCATED
                        when ('0', '0', '00', '000111') => __encoding aarch64_float_convert_fp // FCVT_HS_floatdp1
                        when ('0', '0', '00', '001000') => __encoding aarch64_float_arithmetic_round_frint // FRINTN_S_floatdp1
                        when ('0', '0', '00', '001001') => __encoding aarch64_float_arithmetic_round_frint // FRINTP_S_floatdp1
                        when ('0', '0', '00', '001010') => __encoding aarch64_float_arithmetic_round_frint // FRINTM_S_floatdp1
                        when ('0', '0', '00', '001011') => __encoding aarch64_float_arithmetic_round_frint // FRINTZ_S_floatdp1
                        when ('0', '0', '00', '001100') => __encoding aarch64_float_arithmetic_round_frint // FRINTA_S_floatdp1
                        when ('0', '0', '00', '001101') => __UNALLOCATED
                        when ('0', '0', '00', '001110') => __encoding aarch64_float_arithmetic_round_frint // FRINTX_S_floatdp1
                        when ('0', '0', '00', '001111') => __encoding aarch64_float_arithmetic_round_frint // FRINTI_S_floatdp1
                        when ('0', '0', '00', '010000') => __encoding aarch64_float_arithmetic_round_frint_32_64 // FRINT32Z_S_floatdp1
                        when ('0', '0', '00', '010001') => __encoding aarch64_float_arithmetic_round_frint_32_64 // FRINT32X_S_floatdp1
                        when ('0', '0', '00', '010010') => __encoding aarch64_float_arithmetic_round_frint_32_64 // FRINT64Z_S_floatdp1
                        when ('0', '0', '00', '010011') => __encoding aarch64_float_arithmetic_round_frint_32_64 // FRINT64X_S_floatdp1
                        when ('0', '0', '00', '0101xx') => __UNALLOCATED
                        when ('0', '0', '00', '011xxx') => __UNALLOCATED
                        when ('0', '0', '01', '000000') => __encoding aarch64_float_arithmetic_unary // FMOV_D_floatdp1
                        when ('0', '0', '01', '000001') => __encoding aarch64_float_arithmetic_unary // FABS_D_floatdp1
                        when ('0', '0', '01', '000010') => __encoding aarch64_float_arithmetic_unary // FNEG_D_floatdp1
                        when ('0', '0', '01', '000011') => __encoding aarch64_float_arithmetic_unary // FSQRT_D_floatdp1
                        when ('0', '0', '01', '000100') => __encoding aarch64_float_convert_fp // FCVT_SD_floatdp1
                        when ('0', '0', '01', '000101') => __UNALLOCATED
                        when ('0', '0', '01', '000111') => __encoding aarch64_float_convert_fp // FCVT_HD_floatdp1
                        when ('0', '0', '01', '001000') => __encoding aarch64_float_arithmetic_round_frint // FRINTN_D_floatdp1
                        when ('0', '0', '01', '001001') => __encoding aarch64_float_arithmetic_round_frint // FRINTP_D_floatdp1
                        when ('0', '0', '01', '001010') => __encoding aarch64_float_arithmetic_round_frint // FRINTM_D_floatdp1
                        when ('0', '0', '01', '001011') => __encoding aarch64_float_arithmetic_round_frint // FRINTZ_D_floatdp1
                        when ('0', '0', '01', '001100') => __encoding aarch64_float_arithmetic_round_frint // FRINTA_D_floatdp1
                        when ('0', '0', '01', '001101') => __UNALLOCATED
                        when ('0', '0', '01', '001110') => __encoding aarch64_float_arithmetic_round_frint // FRINTX_D_floatdp1
                        when ('0', '0', '01', '001111') => __encoding aarch64_float_arithmetic_round_frint // FRINTI_D_floatdp1
                        when ('0', '0', '01', '010000') => __encoding aarch64_float_arithmetic_round_frint_32_64 // FRINT32Z_D_floatdp1
                        when ('0', '0', '01', '010001') => __encoding aarch64_float_arithmetic_round_frint_32_64 // FRINT32X_D_floatdp1
                        when ('0', '0', '01', '010010') => __encoding aarch64_float_arithmetic_round_frint_32_64 // FRINT64Z_D_floatdp1
                        when ('0', '0', '01', '010011') => __encoding aarch64_float_arithmetic_round_frint_32_64 // FRINT64X_D_floatdp1
                        when ('0', '0', '01', '0101xx') => __UNALLOCATED
                        when ('0', '0', '01', '011xxx') => __UNALLOCATED
                        when ('0', '0', '10', '0xxxxx') => __UNALLOCATED
                        when ('0', '0', '11', '000000') => __encoding aarch64_float_arithmetic_unary // FMOV_H_floatdp1
                        when ('0', '0', '11', '000001') => __encoding aarch64_float_arithmetic_unary // FABS_H_floatdp1
                        when ('0', '0', '11', '000010') => __encoding aarch64_float_arithmetic_unary // FNEG_H_floatdp1
                        when ('0', '0', '11', '000011') => __encoding aarch64_float_arithmetic_unary // FSQRT_H_floatdp1
                        when ('0', '0', '11', '000100') => __encoding aarch64_float_convert_fp // FCVT_SH_floatdp1
                        when ('0', '0', '11', '000101') => __encoding aarch64_float_convert_fp // FCVT_DH_floatdp1
                        when ('0', '0', '11', '00011x') => __UNALLOCATED
                        when ('0', '0', '11', '001000') => __encoding aarch64_float_arithmetic_round_frint // FRINTN_H_floatdp1
                        when ('0', '0', '11', '001001') => __encoding aarch64_float_arithmetic_round_frint // FRINTP_H_floatdp1
                        when ('0', '0', '11', '001010') => __encoding aarch64_float_arithmetic_round_frint // FRINTM_H_floatdp1
                        when ('0', '0', '11', '001011') => __encoding aarch64_float_arithmetic_round_frint // FRINTZ_H_floatdp1
                        when ('0', '0', '11', '001100') => __encoding aarch64_float_arithmetic_round_frint // FRINTA_H_floatdp1
                        when ('0', '0', '11', '001101') => __UNALLOCATED
                        when ('0', '0', '11', '001110') => __encoding aarch64_float_arithmetic_round_frint // FRINTX_H_floatdp1
                        when ('0', '0', '11', '001111') => __encoding aarch64_float_arithmetic_round_frint // FRINTI_H_floatdp1
                        when ('0', '0', '11', '01xxxx') => __UNALLOCATED
                        when ('1', _, _, _) => __UNALLOCATED
                when ('x0x1', _, '0x', 'x1xx', 'xxxxx1000', _) => // floatcmp
                    __field M 31 +: 1
                    __field S 29 +: 1
                    __field type1 22 +: 2
                    __field Rm 16 +: 5
                    __field op 14 +: 2
                    __field Rn 5 +: 5
                    __field opcode2 0 +: 5
                    case (M, S, type1, op, opcode2) of
                        when (_, _, _, _, 'xxxx1') => __UNALLOCATED
                        when (_, _, _, _, 'xxx1x') => __UNALLOCATED
                        when (_, _, _, _, 'xx1xx') => __UNALLOCATED
                        when (_, _, _, 'x1', _) => __UNALLOCATED
                        when (_, _, _, '1x', _) => __UNALLOCATED
                        when (_, _, '10', _, _) => __UNALLOCATED
                        when (_, '1', _, _, _) => __UNALLOCATED
                        when ('0', '0', '00', '00', '00000') => __encoding aarch64_float_compare_uncond // FCMP_S_floatcmp
                        when ('0', '0', '00', '00', '01000') => __encoding aarch64_float_compare_uncond // FCMP_SZ_floatcmp
                        when ('0', '0', '00', '00', '10000') => __encoding aarch64_float_compare_uncond // FCMPE_S_floatcmp
                        when ('0', '0', '00', '00', '11000') => __encoding aarch64_float_compare_uncond // FCMPE_SZ_floatcmp
                        when ('0', '0', '01', '00', '00000') => __encoding aarch64_float_compare_uncond // FCMP_D_floatcmp
                        when ('0', '0', '01', '00', '01000') => __encoding aarch64_float_compare_uncond // FCMP_DZ_floatcmp
                        when ('0', '0', '01', '00', '10000') => __encoding aarch64_float_compare_uncond // FCMPE_D_floatcmp
                        when ('0', '0', '01', '00', '11000') => __encoding aarch64_float_compare_uncond // FCMPE_DZ_floatcmp
                        when ('0', '0', '11', '00', '00000') => __encoding aarch64_float_compare_uncond // FCMP_H_floatcmp
                        when ('0', '0', '11', '00', '01000') => __encoding aarch64_float_compare_uncond // FCMP_HZ_floatcmp
                        when ('0', '0', '11', '00', '10000') => __encoding aarch64_float_compare_uncond // FCMPE_H_floatcmp
                        when ('0', '0', '11', '00', '11000') => __encoding aarch64_float_compare_uncond // FCMPE_HZ_floatcmp
                        when ('1', _, _, _, _) => __UNALLOCATED
                when ('x0x1', _, '0x', 'x1xx', 'xxxxxx100', _) => // floatimm
                    __field M 31 +: 1
                    __field S 29 +: 1
                    __field type1 22 +: 2
                    __field imm8 13 +: 8
                    __field imm5 5 +: 5
                    __field Rd 0 +: 5
                    case (M, S, type1, imm5) of
                        when (_, _, _, 'xxxx1') => __UNALLOCATED
                        when (_, _, _, 'xxx1x') => __UNALLOCATED
                        when (_, _, _, 'xx1xx') => __UNALLOCATED
                        when (_, _, _, 'x1xxx') => __UNALLOCATED
                        when (_, _, _, '1xxxx') => __UNALLOCATED
                        when (_, _, '10', _) => __UNALLOCATED
                        when (_, '1', _, _) => __UNALLOCATED
                        when ('0', '0', '00', '00000') => __encoding aarch64_float_move_fp_imm // FMOV_S_floatimm
                        when ('0', '0', '01', '00000') => __encoding aarch64_float_move_fp_imm // FMOV_D_floatimm
                        when ('0', '0', '11', '00000') => __encoding aarch64_float_move_fp_imm // FMOV_H_floatimm
                        when ('1', _, _, _) => __UNALLOCATED
                when ('x0x1', _, '0x', 'x1xx', 'xxxxxxx01', _) => // floatccmp
                    __field M 31 +: 1
                    __field S 29 +: 1
                    __field type1 22 +: 2
                    __field Rm 16 +: 5
                    __field cond 12 +: 4
                    __field Rn 5 +: 5
                    __field op 4 +: 1
                    __field nzcv 0 +: 4
                    case (M, S, type1, op) of
                        when (_, _, '10', _) => __UNALLOCATED
                        when (_, '1', _, _) => __UNALLOCATED
                        when ('0', '0', '00', '0') => __encoding aarch64_float_compare_cond // FCCMP_S_floatccmp
                        when ('0', '0', '00', '1') => __encoding aarch64_float_compare_cond // FCCMPE_S_floatccmp
                        when ('0', '0', '01', '0') => __encoding aarch64_float_compare_cond // FCCMP_D_floatccmp
                        when ('0', '0', '01', '1') => __encoding aarch64_float_compare_cond // FCCMPE_D_floatccmp
                        when ('0', '0', '11', '0') => __encoding aarch64_float_compare_cond // FCCMP_H_floatccmp
                        when ('0', '0', '11', '1') => __encoding aarch64_float_compare_cond // FCCMPE_H_floatccmp
                        when ('1', _, _, _) => __UNALLOCATED
                when ('x0x1', _, '0x', 'x1xx', 'xxxxxxx10', _) => // floatdp2
                    __field M 31 +: 1
                    __field S 29 +: 1
                    __field type1 22 +: 2
                    __field Rm 16 +: 5
                    __field opcode 12 +: 4
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (M, S, type1, opcode) of
                        when (_, _, _, '1xx1') => __UNALLOCATED
                        when (_, _, _, '1x1x') => __UNALLOCATED
                        when (_, _, _, '11xx') => __UNALLOCATED
                        when (_, _, '10', _) => __UNALLOCATED
                        when (_, '1', _, _) => __UNALLOCATED
                        when ('0', '0', '00', '0000') => __encoding aarch64_float_arithmetic_mul_product // FMUL_S_floatdp2
                        when ('0', '0', '00', '0001') => __encoding aarch64_float_arithmetic_div // FDIV_S_floatdp2
                        when ('0', '0', '00', '0010') => __encoding aarch64_float_arithmetic_add_sub // FADD_S_floatdp2
                        when ('0', '0', '00', '0011') => __encoding aarch64_float_arithmetic_add_sub // FSUB_S_floatdp2
                        when ('0', '0', '00', '0100') => __encoding aarch64_float_arithmetic_max_min // FMAX_S_floatdp2
                        when ('0', '0', '00', '0101') => __encoding aarch64_float_arithmetic_max_min // FMIN_S_floatdp2
                        when ('0', '0', '00', '0110') => __encoding aarch64_float_arithmetic_max_min // FMAXNM_S_floatdp2
                        when ('0', '0', '00', '0111') => __encoding aarch64_float_arithmetic_max_min // FMINNM_S_floatdp2
                        when ('0', '0', '00', '1000') => __encoding aarch64_float_arithmetic_mul_product // FNMUL_S_floatdp2
                        when ('0', '0', '01', '0000') => __encoding aarch64_float_arithmetic_mul_product // FMUL_D_floatdp2
                        when ('0', '0', '01', '0001') => __encoding aarch64_float_arithmetic_div // FDIV_D_floatdp2
                        when ('0', '0', '01', '0010') => __encoding aarch64_float_arithmetic_add_sub // FADD_D_floatdp2
                        when ('0', '0', '01', '0011') => __encoding aarch64_float_arithmetic_add_sub // FSUB_D_floatdp2
                        when ('0', '0', '01', '0100') => __encoding aarch64_float_arithmetic_max_min // FMAX_D_floatdp2
                        when ('0', '0', '01', '0101') => __encoding aarch64_float_arithmetic_max_min // FMIN_D_floatdp2
                        when ('0', '0', '01', '0110') => __encoding aarch64_float_arithmetic_max_min // FMAXNM_D_floatdp2
                        when ('0', '0', '01', '0111') => __encoding aarch64_float_arithmetic_max_min // FMINNM_D_floatdp2
                        when ('0', '0', '01', '1000') => __encoding aarch64_float_arithmetic_mul_product // FNMUL_D_floatdp2
                        when ('0', '0', '11', '0000') => __encoding aarch64_float_arithmetic_mul_product // FMUL_H_floatdp2
                        when ('0', '0', '11', '0001') => __encoding aarch64_float_arithmetic_div // FDIV_H_floatdp2
                        when ('0', '0', '11', '0010') => __encoding aarch64_float_arithmetic_add_sub // FADD_H_floatdp2
                        when ('0', '0', '11', '0011') => __encoding aarch64_float_arithmetic_add_sub // FSUB_H_floatdp2
                        when ('0', '0', '11', '0100') => __encoding aarch64_float_arithmetic_max_min // FMAX_H_floatdp2
                        when ('0', '0', '11', '0101') => __encoding aarch64_float_arithmetic_max_min // FMIN_H_floatdp2
                        when ('0', '0', '11', '0110') => __encoding aarch64_float_arithmetic_max_min // FMAXNM_H_floatdp2
                        when ('0', '0', '11', '0111') => __encoding aarch64_float_arithmetic_max_min // FMINNM_H_floatdp2
                        when ('0', '0', '11', '1000') => __encoding aarch64_float_arithmetic_mul_product // FNMUL_H_floatdp2
                        when ('1', _, _, _) => __UNALLOCATED
                when ('x0x1', _, '0x', 'x1xx', 'xxxxxxx11', _) => // floatsel
                    __field M 31 +: 1
                    __field S 29 +: 1
                    __field type1 22 +: 2
                    __field Rm 16 +: 5
                    __field cond 12 +: 4
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (M, S, type1) of
                        when (_, _, '10') => __UNALLOCATED
                        when (_, '1', _) => __UNALLOCATED
                        when ('0', '0', '00') => __encoding aarch64_float_move_fp_select // FCSEL_S_floatsel
                        when ('0', '0', '01') => __encoding aarch64_float_move_fp_select // FCSEL_D_floatsel
                        when ('0', '0', '11') => __encoding aarch64_float_move_fp_select // FCSEL_H_floatsel
                        when ('1', _, _) => __UNALLOCATED
                when ('x0x1', _, '1x', _, _, _) => // floatdp3
                    __field M 31 +: 1
                    __field S 29 +: 1
                    __field type1 22 +: 2
                    __field o1 21 +: 1
                    __field Rm 16 +: 5
                    __field o0 15 +: 1
                    __field Ra 10 +: 5
                    __field Rn 5 +: 5
                    __field Rd 0 +: 5
                    case (M, S, type1, o1, o0) of
                        when (_, _, '10', _, _) => __UNALLOCATED
                        when (_, '1', _, _, _) => __UNALLOCATED
                        when ('0', '0', '00', '0', '0') => __encoding aarch64_float_arithmetic_mul_add_sub // FMADD_S_floatdp3
                        when ('0', '0', '00', '0', '1') => __encoding aarch64_float_arithmetic_mul_add_sub // FMSUB_S_floatdp3
                        when ('0', '0', '00', '1', '0') => __encoding aarch64_float_arithmetic_mul_add_sub // FNMADD_S_floatdp3
                        when ('0', '0', '00', '1', '1') => __encoding aarch64_float_arithmetic_mul_add_sub // FNMSUB_S_floatdp3
                        when ('0', '0', '01', '0', '0') => __encoding aarch64_float_arithmetic_mul_add_sub // FMADD_D_floatdp3
                        when ('0', '0', '01', '0', '1') => __encoding aarch64_float_arithmetic_mul_add_sub // FMSUB_D_floatdp3
                        when ('0', '0', '01', '1', '0') => __encoding aarch64_float_arithmetic_mul_add_sub // FNMADD_D_floatdp3
                        when ('0', '0', '01', '1', '1') => __encoding aarch64_float_arithmetic_mul_add_sub // FNMSUB_D_floatdp3
                        when ('0', '0', '11', '0', '0') => __encoding aarch64_float_arithmetic_mul_add_sub // FMADD_H_floatdp3
                        when ('0', '0', '11', '0', '1') => __encoding aarch64_float_arithmetic_mul_add_sub // FMSUB_H_floatdp3
                        when ('0', '0', '11', '1', '0') => __encoding aarch64_float_arithmetic_mul_add_sub // FNMADD_H_floatdp3
                        when ('0', '0', '11', '1', '1') => __encoding aarch64_float_arithmetic_mul_add_sub // FNMSUB_H_floatdp3
                        when ('1', _, _, _, _) => __UNALLOCATED
__decode T32
    // T32
    case (29 +: 3, 27 +: 2, 0 +: 27) of
        when (!'111', _, _) =>
            // n
            case (26 +: 6, 0 +: 26) of
                when ('00xxxx', _) =>
                    // sftdpi
                    case (30 +: 2, 29 +: 1, 27 +: 2, 26 +: 1, 0 +: 26) of
                        when (_, '0', '11', '0', _) => // addsub16_3l
                            __field S 25 +: 1
                            __field Rm 22 +: 3
                            __field Rn 19 +: 3
                            __field Rd 16 +: 3
                            case (S) of
                                when ('0') => __encoding aarch32_ADD_r_T1_A // ADD_r_T1
                                when ('1') => __encoding aarch32_SUB_r_T1_A // SUB_r_T1
                        when (_, '0', '11', '1', _) => // addsub16_2l_imm
                            __field S 25 +: 1
                            __field imm3 22 +: 3
                            __field Rn 19 +: 3
                            __field Rd 16 +: 3
                            case (S) of
                                when ('0') => __encoding aarch32_ADD_i_T1_A // ADD_i_T1
                                when ('1') => __encoding aarch32_SUB_i_T1_A // SUB_i_T1
                        when (_, '0', !'11', _, _) => // shift16_imm
                            __field op 27 +: 2
                            __field imm5 22 +: 5
                            __field Rm 19 +: 3
                            __field Rd 16 +: 3
                            case () of
                                when () => __encoding aarch32_MOV_r_T2_A // MOV_r_T2
                        when (_, '1', _, _, _) => // addsub16_1l_imm
                            __field op 27 +: 2
                            __field Rd 24 +: 3
                            __field imm8 16 +: 8
                            case (op) of
                                when ('00') => __encoding aarch32_MOV_i_T1_A // MOV_i_T1
                                when ('01') => __encoding aarch32_CMP_i_T1_A // CMP_i_T1
                                when ('10') => __encoding aarch32_ADD_i_T2_A // ADD_i_T2
                                when ('11') => __encoding aarch32_SUB_i_T2_A // SUB_i_T2
                when ('010000', _) => // dpint16_2l
                    __field op 22 +: 4
                    __field Rs 19 +: 3
                    __field Rd 16 +: 3
                    case (op) of
                        when ('0000') => __encoding aarch32_AND_r_T1_A // AND_r_T1
                        when ('0001') => __encoding aarch32_EOR_r_T1_A // EOR_r_T1
                        when ('0010') => __encoding aarch32_MOV_rr_T1_A // MOV_rr_T1_LSL
                        when ('0011') => __encoding aarch32_MOV_rr_T1_A // MOV_rr_T1_LSR
                        when ('0100') => __encoding aarch32_MOV_rr_T1_A // MOV_rr_T1_ASR
                        when ('0101') => __encoding aarch32_ADC_r_T1_A // ADC_r_T1
                        when ('0110') => __encoding aarch32_SBC_r_T1_A // SBC_r_T1
                        when ('0111') => __encoding aarch32_MOV_rr_T1_A // MOV_rr_T1_ROR
                        when ('1000') => __encoding aarch32_TST_r_T1_A // TST_r_T1
                        when ('1001') => __encoding aarch32_RSB_i_T1_A // RSB_i_T1
                        when ('1010') => __encoding aarch32_CMP_r_T1_A // CMP_r_T1
                        when ('1011') => __encoding aarch32_CMN_r_T1_A // CMN_r_T1
                        when ('1100') => __encoding aarch32_ORR_r_T1_A // ORR_r_T1
                        when ('1101') => __encoding aarch32_MUL_T1_A // MUL_T1
                        when ('1110') => __encoding aarch32_BIC_r_T1_A // BIC_r_T1
                        when ('1111') => __encoding aarch32_MVN_r_T1_A // MVN_r_T1
                when ('010001', _) =>
                    // spcd
                    case (26 +: 6, 24 +: 2, 0 +: 24) of
                        when (_, '11', _) => // bx16
                            __field L 23 +: 1
                            __field Rm 19 +: 4
                            case (L) of
                                when ('0') => __encoding aarch32_BX_T1_A // BX_T1
                                when ('1') => __encoding aarch32_BLX_r_T1_A // BLX_r_T1
                        when (_, !'11', _) => // addsub16_2h
                            __field op 24 +: 2
                            __field D 23 +: 1
                            __field Rs 19 +: 4
                            __field Rd 16 +: 3
                            case (op, D:Rd, Rs) of
                                when ('00', !'1101', !'1101') => __encoding aarch32_ADD_r_T2_A // ADD_r_T2
                                when ('00', _, '1101') => __encoding aarch32_ADD_SP_r_A1_A
                                when ('00', '1101', !'1101') => __encoding aarch32_ADD_SP_r_A1_A
                                when ('01', _, _) => __encoding aarch32_CMP_r_T2_A // CMP_r_T2
                                when ('10', _, _) => __encoding aarch32_MOV_r_T1_A // MOV_r_T1
                when ('01001x', _) => // ldlit16
                    __field Rt 24 +: 3
                    __field imm8 16 +: 8
                    case () of
                        when () => __encoding aarch32_LDR_l_T1_A // LDR_l_T1
                when ('0101xx', _) => // ldst16_reg
                    __field L 27 +: 1
                    __field B 26 +: 1
                    __field H 25 +: 1
                    __field Rm 22 +: 3
                    __field Rn 19 +: 3
                    __field Rt 16 +: 3
                    case (L, B, H) of
                        when ('0', '0', '0') => __encoding aarch32_STR_r_T1_A // STR_r_T1
                        when ('0', '0', '1') => __encoding aarch32_STRH_r_T1_A // STRH_r_T1
                        when ('0', '1', '0') => __encoding aarch32_STRB_r_T1_A // STRB_r_T1
                        when ('0', '1', '1') => __encoding aarch32_LDRSB_r_T1_A // LDRSB_r_T1
                        when ('1', '0', '0') => __encoding aarch32_LDR_r_T1_A // LDR_r_T1
                        when ('1', '0', '1') => __encoding aarch32_LDRH_r_T1_A // LDRH_r_T1
                        when ('1', '1', '0') => __encoding aarch32_LDRB_r_T1_A // LDRB_r_T1
                        when ('1', '1', '1') => __encoding aarch32_LDRSH_r_T1_A // LDRSH_r_T1
                when ('011xxx', _) => // ldst16_imm
                    __field B 28 +: 1
                    __field L 27 +: 1
                    __field imm5 22 +: 5
                    __field Rn 19 +: 3
                    __field Rt 16 +: 3
                    case (B, L) of
                        when ('0', '0') => __encoding aarch32_STR_i_T1_A // STR_i_T1
                        when ('0', '1') => __encoding aarch32_LDR_i_T1_A // LDR_i_T1
                        when ('1', '0') => __encoding aarch32_STRB_i_T1_A // STRB_i_T1
                        when ('1', '1') => __encoding aarch32_LDRB_i_T1_A // LDRB_i_T1
                when ('1000xx', _) => // ldsth16_imm
                    __field L 27 +: 1
                    __field imm5 22 +: 5
                    __field Rn 19 +: 3
                    __field Rt 16 +: 3
                    case (L) of
                        when ('0') => __encoding aarch32_STRH_i_T1_A // STRH_i_T1
                        when ('1') => __encoding aarch32_LDRH_i_T1_A // LDRH_i_T1
                when ('1001xx', _) => // ldst16_sp
                    __field L 27 +: 1
                    __field Rt 24 +: 3
                    __field imm8 16 +: 8
                    case (L) of
                        when ('0') => __encoding aarch32_STR_i_T2_A // STR_i_T2
                        when ('1') => __encoding aarch32_LDR_i_T2_A // LDR_i_T2
                when ('1010xx', _) => // addpcsp16
                    __field SP 27 +: 1
                    __field Rd 24 +: 3
                    __field imm8 16 +: 8
                    case (SP) of
                        when ('0') => __encoding aarch32_ADR_T1_A // ADR_T1
                        when ('1') => __encoding aarch32_ADD_SP_i_T1_A // ADD_SP_i_T1
                when ('1011xx', _) =>
                    // misc16
                    case (28 +: 4, 24 +: 4, 22 +: 2, 21 +: 1, 20 +: 1, 16 +: 4, 0 +: 16) of
                        when (_, '0000', _, _, _, _, _) => // adjsp16
                            __field S 23 +: 1
                            __field imm7 16 +: 7
                            case (S) of
                                when ('0') => __encoding aarch32_ADD_SP_i_T2_A // ADD_SP_i_T2
                                when ('1') => __encoding aarch32_SUB_SP_i_T1_A // SUB_SP_i_T1
                        when (_, '0010', _, _, _, _, _) => // ext16
                            __field U 23 +: 1
                            __field B 22 +: 1
                            __field Rm 19 +: 3
                            __field Rd 16 +: 3
                            case (U, B) of
                                when ('0', '0') => __encoding aarch32_SXTH_T1_A // SXTH_T1
                                when ('0', '1') => __encoding aarch32_SXTB_T1_A // SXTB_T1
                                when ('1', '0') => __encoding aarch32_UXTH_T1_A // UXTH_T1
                                when ('1', '1') => __encoding aarch32_UXTB_T1_A // UXTB_T1
                        when (_, '0110', '00', '0', _, _, _) => // setpan16
                            __field imm1 19 +: 1
                            case () of
                                when () => __encoding aarch32_SETPAN_T1_A // SETPAN_T1
                        when (_, '0110', '00', '1', _, _, _) => __UNPREDICTABLE
                        when (_, '0110', '01', _, _, _, _) => // cps16
                            __field op 21 +: 1
                            __field flags 16 +: 5
                            case (op, flags) of
                                when ('0', _) => __encoding aarch32_SETEND_T1_A // SETEND_T1
                                when ('1', _) => __encoding aarch32_CPS_T1_AS // CPSID_T1_AS
                        when (_, '0110', '1x', _, _, _, _) => __UNPREDICTABLE
                        when (_, '0111', _, _, _, _, _) => __UNPREDICTABLE
                        when (_, '1000', _, _, _, _, _) => __UNPREDICTABLE
                        when (_, '1010', '10', _, _, _, _) => // hlt16
                            __field imm6 16 +: 6
                            case () of
                                when () => __encoding aarch32_HLT_T1_A // HLT_T1
                        when (_, '1010', !'10', _, _, _, _) => // rev16
                            __field op 22 +: 2
                            __field Rm 19 +: 3
                            __field Rd 16 +: 3
                            case (op) of
                                when ('00') => __encoding aarch32_REV_T1_A // REV_T1
                                when ('01') => __encoding aarch32_REV16_T1_A // REV16_T1
                                when ('11') => __encoding aarch32_REVSH_T1_A // REVSH_T1
                        when (_, '1110', _, _, _, _, _) => // bkpt16
                            __field imm8 16 +: 8
                            case () of
                                when () => __encoding aarch32_BKPT_T1_A // BKPT_T1
                        when (_, '1111', _, _, _, '0000', _) => // hints16
                            __field hint 20 +: 4
                            case (hint) of
                                when ('0000') => __encoding aarch32_NOP_T1_A // NOP_T1
                                when ('0001') => __encoding aarch32_YIELD_T1_A // YIELD_T1
                                when ('0010') => __encoding aarch32_WFE_T1_A // WFE_T1
                                when ('0011') => __encoding aarch32_WFI_T1_A // WFI_T1
                                when ('0100') => __encoding aarch32_SEV_T1_A // SEV_T1
                                when ('0101') => __encoding aarch32_SEVL_T1_A // SEVL_T1
                                when ('011x') => __NOP
                                when ('1xxx') => __NOP
                        when (_, '1111', _, _, _, !'0000', _) => // it16
                            __field firstcond 20 +: 4
                            __field mask 16 +: 4
                            case () of
                                when () => __encoding aarch32_IT_T1_A // IT_T1
                        when (_, 'x0x1', _, _, _, _, _) => // cbznz16
                            __field op 27 +: 1
                            __field i 25 +: 1
                            __field imm5 19 +: 5
                            __field Rn 16 +: 3
                            case () of
                                when () => __encoding aarch32_CBNZ_T1_A // CBNZ_T1
                        when (_, 'x10x', _, _, _, _, _) => // pushpop16
                            __field L 27 +: 1
                            __field P 24 +: 1
                            __field register_list 16 +: 8
                            case (L) of
                                when ('0') => __encoding aarch32_PUSH_T1_A // PUSH_T1
                                when ('1') => __encoding aarch32_POP_T1_A // POP_T1
                when ('1100xx', _) => // ldstm16
                    __field L 27 +: 1
                    __field Rn 24 +: 3
                    __field register_list 16 +: 8
                    case (L) of
                        when ('0') => __encoding aarch32_STM_T1_A // STM_T1
                        when ('1') => __encoding aarch32_LDM_T1_A // LDM_T1
                when ('1101xx', _) =>
                    // brc
                    case (28 +: 4, 24 +: 4, 0 +: 24) of
                        when (_, '111x', _) => // except16
                            __field S 24 +: 1
                            __field imm8 16 +: 8
                            case (S) of
                                when ('0') => __encoding aarch32_UDF_T1_A // UDF_T1
                                when ('1') => __encoding aarch32_SVC_T1_A // SVC_T1
                        when (_, !'111x', _) => // bcond16
                            __field cond 24 +: 4
                            __field imm8 16 +: 8
                            case () of
                                when () => __encoding aarch32_B_T1_A // B_T1
        when ('111', '00', _) => // b16
            __field imm11 16 +: 11
            case () of
                when () => __encoding aarch32_B_T2_A // B_T2
        when ('111', !'00', _) =>
            // w
            case (29 +: 3, 25 +: 4, 20 +: 5, 16 +: 4, 15 +: 1, 0 +: 15) of
                when (_, 'x11x', _, _, _, _) =>
                    // cpaf
                    case (29 +: 3, 28 +: 1, 26 +: 2, 24 +: 2, 12 +: 12, 9 +: 3, 5 +: 4, 4 +: 1, 0 +: 4) of
                        when (_, _, _, '0x', _, '111', _, _, _) =>
                            // sysldst_mov64
                            case (29 +: 3, 28 +: 1, 25 +: 3, 21 +: 4, 12 +: 9, 9 +: 3, 0 +: 9) of
                                when (_, _, _, '00x0', _, _, _) => // cp_mov64
                                    __field o0 28 +: 1
                                    __field D 22 +: 1
                                    __field L 20 +: 1
                                    __field Rt2 16 +: 4
                                    __field Rt 12 +: 4
                                    __field cp15 8 +: 1
                                    __field opc1 4 +: 4
                                    __field CRm 0 +: 4
                                    case (o0, D, L) of
                                        when ('0', '0', _) => __UNALLOCATED
                                        when ('0', '1', '0') => __encoding aarch32_MCRR_T1A1_A // MCRR_T1
                                        when ('0', '1', '1') => __encoding aarch32_MRRC_T1A1_A // MRRC_T1
                                        when ('1', '0', _) => __UNALLOCATED
                                        when ('1', '1', _) => __UNALLOCATED
                                when (_, _, _, !'00x0', _, _, _) => // cp_ldst
                                    __field o0 28 +: 1
                                    __field P 24 +: 1
                                    __field U 23 +: 1
                                    __field D 22 +: 1
                                    __field W 21 +: 1
                                    __field L 20 +: 1
                                    __field Rn 16 +: 4
                                    __field CRd 12 +: 4
                                    __field cp15 8 +: 1
                                    __field imm8 0 +: 8
                                    case (o0, P:U:W, D, L, Rn, CRd, cp15) of
                                        when (_, !'000', _, _, _, !'0101', '0') => __UNALLOCATED
                                        when (_, !'000', _, _, _, _, '1') => __UNALLOCATED
                                        when (_, !'000', '1', _, _, '0101', '0') => __UNALLOCATED
                                        when ('0', !'000', '0', '1', '1111', '0101', '0') => __encoding aarch32_LDC_l_T1A1_A // LDC_l_T1
                                        when ('0', '0x1', '0', '0', _, '0101', '0') => __encoding aarch32_STC_T1A1_A // STC_T1_post
                                        when ('0', '0x1', '0', '1', !'1111', '0101', '0') => __encoding aarch32_LDC_i_T1A1_A // LDC_i_T1_post
                                        when ('0', '010', '0', '0', _, '0101', '0') => __encoding aarch32_STC_T1A1_A // STC_T1_unind
                                        when ('0', '010', '0', '1', !'1111', '0101', '0') => __encoding aarch32_LDC_i_T1A1_A // LDC_i_T1_unind
                                        when ('0', '1x0', '0', '0', _, '0101', '0') => __encoding aarch32_STC_T1A1_A // STC_T1_off
                                        when ('0', '1x0', '0', '1', !'1111', '0101', '0') => __encoding aarch32_LDC_i_T1A1_A // LDC_i_T1_off
                                        when ('0', '1x1', '0', '0', _, '0101', '0') => __encoding aarch32_STC_T1A1_A // STC_T1_pre
                                        when ('0', '1x1', '0', '1', !'1111', '0101', '0') => __encoding aarch32_LDC_i_T1A1_A // LDC_i_T1_pre
                                        when ('1', !'000', '0', _, _, '0101', '0') => __UNALLOCATED
                        when (_, _, _, '10', _, '10x', _, '0', _) =>
                            // fpdp
                            case (29 +: 3, 28 +: 1, 24 +: 4, 20 +: 4, 16 +: 4, 12 +: 4, 10 +: 2, 8 +: 2, 7 +: 1, 6 +: 1, 5 +: 1, 4 +: 1, 0 +: 4) of
                                when (_, '0', _, '1x11', _, _, _, _, _, '1', _, _, _) => // fp_2r
                                    __field D 22 +: 1
                                    __field o1 19 +: 1
                                    __field opc2 16 +: 3
                                    __field Vd 12 +: 4
                                    __field size 8 +: 2
                                    __field o3 7 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case (o1, opc2, size, o3) of
                                        when (_, _, '00', _) => __UNALLOCATED
                                        when ('0', '000', '01', '0') => __UNALLOCATED
                                        when ('0', '000', _, '1') => __encoding aarch32_VABS_T2_A // VABS_T2_D
                                        when ('0', '000', '10', '0') => __encoding aarch32_VMOV_r_T2A2_A // VMOV_r_T2_S
                                        when ('0', '000', '11', '0') => __encoding aarch32_VMOV_r_T2A2_A // VMOV_r_T2_D
                                        when ('0', '001', _, '0') => __encoding aarch32_VNEG_T2_A // VNEG_T2_D
                                        when ('0', '001', _, '1') => __encoding aarch32_VSQRT_T1_A // VSQRT_T1_D
                                        when ('0', '01x', '01', _) => __UNALLOCATED
                                        when ('0', '010', _, '0') => __encoding aarch32_VCVTB_T1A1_A // VCVTB_T1_DH
                                        when ('0', '010', _, '1') => __encoding aarch32_VCVTB_T1A1_A // VCVTT_T1_DH
                                        when ('0', '011', _, '0') => __encoding aarch32_VCVTB_T1A1_A // VCVTB_T1_HD
                                        when ('0', '011', _, '1') => __encoding aarch32_VCVTB_T1A1_A // VCVTT_T1_HD
                                        when ('0', '100', _, '0') => __encoding aarch32_VCMP_A1_A
                                        when ('0', '100', _, '1') => __encoding aarch32_VCMP_A1_A
                                        when ('0', '101', _, '0') => __encoding aarch32_VCMP_A1_A
                                        when ('0', '101', _, '1') => __encoding aarch32_VCMP_A1_A
                                        when ('0', '110', _, '0') => __encoding aarch32_VRINTZ_vfp_T1_A // VRINTR_vfp_T1_D
                                        when ('0', '110', _, '1') => __encoding aarch32_VRINTZ_vfp_T1_A // VRINTZ_vfp_T1_D
                                        when ('0', '111', _, '0') => __encoding aarch32_VRINTX_vfp_T1_A // VRINTX_vfp_T1_D
                                        when ('0', '111', '01', '1') => __UNALLOCATED
                                        when ('0', '111', '10', '1') => __encoding aarch32_VCVT_ds_T1A1_A // VCVT_ds_T1
                                        when ('0', '111', '11', '1') => __encoding aarch32_VCVT_ds_T1A1_A // VCVT_sd_T1
                                        when ('1', '000', _, _) => __encoding aarch32_VCVT_iv_T1_A // VCVT_vi_T1_D
                                        when ('1', '001', '01', _) => __UNALLOCATED
                                        when ('1', '001', '10', _) => __UNALLOCATED
                                        when ('1', '001', '11', '0') => __UNALLOCATED
                                        when ('1', '001', '11', '1') => __encoding aarch32_VJCVT_T1_A // VJCVT_T1
                                        when ('1', '01x', _, _) => __encoding aarch32_VCVT_xv_T1_A // VCVT_toxv_T1_D
                                        when ('1', '100', _, '0') => __encoding aarch32_VCVT_iv_T1_A // VCVTR_uiv_T1_D
                                        when ('1', '100', _, '1') => __encoding aarch32_VCVT_iv_T1_A // VCVT_uiv_T1_D
                                        when ('1', '101', _, '0') => __encoding aarch32_VCVT_iv_T1_A // VCVTR_siv_T1_D
                                        when ('1', '101', _, '1') => __encoding aarch32_VCVT_iv_T1_A // VCVT_siv_T1_D
                                        when ('1', '11x', _, _) => __encoding aarch32_VCVT_xv_T1_A // VCVT_xv_T1_D
                                when (_, '0', _, '1x11', _, _, _, _, _, '0', _, _, _) => // fp_movi
                                    __field D 22 +: 1
                                    __field imm4H 16 +: 4
                                    __field Vd 12 +: 4
                                    __field size 8 +: 2
                                    __field imm4L 0 +: 4
                                    case (size) of
                                        when ('00') => __UNALLOCATED
                                        when ('01') => __encoding aarch32_VMOV_i_T2_A // VMOV_i_T2_H
                                        when ('10') => __encoding aarch32_VMOV_i_T2_A // VMOV_i_T2_S
                                        when ('11') => __encoding aarch32_VMOV_i_T2_A // VMOV_i_T2_D
                                when (_, '0', _, !'1x11', _, _, _, _, _, _, _, _, _) => // fp_3r
                                    __field o0 23 +: 1
                                    __field D 22 +: 1
                                    __field o1 20 +: 2
                                    __field Vn 16 +: 4
                                    __field Vd 12 +: 4
                                    __field size 8 +: 2
                                    __field N 7 +: 1
                                    __field o2 6 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case (o0:o1, size, o2) of
                                        when (!'111', '00', _) => __UNALLOCATED
                                        when ('000', _, '0') => __encoding aarch32_VMLA_f_T2_A // VMLA_f_T2_D
                                        when ('000', _, '1') => __encoding aarch32_VMLA_f_T2_A // VMLS_f_T2_D
                                        when ('001', _, '0') => __encoding aarch32_VNMLA_T1_A // VNMLS_T1_D
                                        when ('001', _, '1') => __encoding aarch32_VNMLA_T1_A // VNMLA_T1_D
                                        when ('010', _, '0') => __encoding aarch32_VMUL_f_T2_A // VMUL_f_T2_D
                                        when ('010', _, '1') => __encoding aarch32_VNMLA_T2_A // VNMUL_T1_D
                                        when ('011', _, '0') => __encoding aarch32_VADD_f_T2_A // VADD_f_T2_D
                                        when ('011', _, '1') => __encoding aarch32_VSUB_f_T2_A // VSUB_f_T2_D
                                        when ('100', _, '0') => __encoding aarch32_VDIV_T1_A // VDIV_T1_D
                                        when ('101', _, '0') => __encoding aarch32_VFNMA_T1_A // VFNMS_T1_D
                                        when ('101', _, '1') => __encoding aarch32_VFNMA_T1_A // VFNMA_T1_D
                                        when ('110', _, '0') => __encoding aarch32_VFMA_T2_A // VFMA_T2_D
                                        when ('110', _, '1') => __encoding aarch32_VFMA_T2_A // VFMS_T2_D
                                when (_, '1', _, '0xxx', _, _, _, !'00', _, '0', _, _, _) => // fp_csel
                                    __field D 22 +: 1
                                    __field cc 20 +: 2
                                    __field Vn 16 +: 4
                                    __field Vd 12 +: 4
                                    __field size 8 +: 2
                                    __field N 7 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case (cc, size) of
                                        when ('00', _) => __encoding aarch32_VSEL_T1_A // VSELEQ_T1_D
                                        when ('01', _) => __encoding aarch32_VSEL_T1_A // VSELVS_T1_D
                                        when (_, '01') => __UNALLOCATED
                                        when ('10', _) => __encoding aarch32_VSEL_T1_A // VSELGE_T1_D
                                        when ('11', _) => __encoding aarch32_VSEL_T1_A // VSELGT_T1_D
                                when (_, '1', _, '1x00', _, _, _, !'00', _, _, _, _, _) => // fp_minmax
                                    __field D 22 +: 1
                                    __field Vn 16 +: 4
                                    __field Vd 12 +: 4
                                    __field size 8 +: 2
                                    __field N 7 +: 1
                                    __field op 6 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case (size, op) of
                                        when (_, '0') => __encoding aarch32_VMAXNM_T2_A // VMAXNM_T2_D
                                        when ('01', _) => __UNALLOCATED
                                        when (_, '1') => __encoding aarch32_VMAXNM_T2_A // VMINNM_T2_D
                                when (_, '1', _, '1x11', '0000', _, _, !'00', _, '1', _, _, _) => // fp_extins
                                    __field D 22 +: 1
                                    __field Vd 12 +: 4
                                    __field size 8 +: 2
                                    __field op 7 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case (size, op) of
                                        when ('01', _) => __UNALLOCATED
                                        when ('10', '0') => __encoding aarch32_VMOVX_T1_A // VMOVX_T1
                                        when ('10', '1') => __encoding aarch32_VINS_T1_A // VINS_T1
                                        when ('11', _) => __UNALLOCATED
                                when (_, '1', _, '1x11', '1xxx', _, _, !'00', _, '1', _, _, _) => // fp_toint
                                    __field D 22 +: 1
                                    __field o1 18 +: 1
                                    __field RM 16 +: 2
                                    __field Vd 12 +: 4
                                    __field size 8 +: 2
                                    __field op 7 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case (o1, RM, size) of
                                        when ('0', '00', _) => __encoding aarch32_VRINTA_vfp_T1_A // VRINTA_vfp_T1_D
                                        when ('0', '01', _) => __encoding aarch32_VRINTA_vfp_T1_A // VRINTN_vfp_T1_D
                                        when (_, _, '01') => __UNALLOCATED
                                        when ('0', '10', _) => __encoding aarch32_VRINTA_vfp_T1_A // VRINTP_vfp_T1_D
                                        when ('0', '11', _) => __encoding aarch32_VRINTA_vfp_T1_A // VRINTM_vfp_T1_D
                                        when ('1', '00', _) => __encoding aarch32_VCVTA_vfp_T1_A // VCVTA_vfp_T1_D
                                        when ('1', '01', _) => __encoding aarch32_VCVTA_vfp_T1_A // VCVTN_vfp_T1_D
                                        when ('1', '10', _) => __encoding aarch32_VCVTA_vfp_T1_A // VCVTP_vfp_T1_D
                                        when ('1', '11', _) => __encoding aarch32_VCVTA_vfp_T1_A // VCVTM_vfp_T1_D
                        when (_, _, _, '10', _, '111', _, '1', _) => // cp_mov32
                            __field o0 28 +: 1
                            __field opc1 21 +: 3
                            __field L 20 +: 1
                            __field CRn 16 +: 4
                            __field Rt 12 +: 4
                            __field cp15 8 +: 1
                            __field opc2 5 +: 3
                            __field CRm 0 +: 4
                            case (o0, L) of
                                when ('0', '0') => __encoding aarch32_MCR_T1A1_A // MCR_T1
                                when ('0', '1') => __encoding aarch32_MRC_T1A1_A // MRC_T1
                                when ('1', _) => __UNALLOCATED
                        when (_, _, _, '11', _, _, _, _, _) =>
                            // simddp
                            case (29 +: 3, 28 +: 1, 24 +: 4, 23 +: 1, 5 +: 18, 4 +: 1, 0 +: 4) of
                                when (_, _, _, '0', _, _, _) => // simd_3same
                                    __field U 28 +: 1
                                    __field D 22 +: 1
                                    __field size 20 +: 2
                                    __field Vn 16 +: 4
                                    __field Vd 12 +: 4
                                    __field opc 8 +: 4
                                    __field N 7 +: 1
                                    __field Q 6 +: 1
                                    __field M 5 +: 1
                                    __field o1 4 +: 1
                                    __field Vm 0 +: 4
                                    case (U, size, opc, Q, o1) of
                                        when ('0', '0x', '1100', _, '1') => __encoding aarch32_VFMA_T1_A // VFMA_T1_Q
                                        when ('0', '0x', '1101', _, '0') => __encoding aarch32_VADD_f_T1_A // VADD_f_T1_Q
                                        when ('0', '0x', '1101', _, '1') => __encoding aarch32_VMLA_f_T1_A // VMLA_f_T1_Q
                                        when ('0', '0x', '1110', _, '0') => __encoding aarch32_VCEQ_r_T1A1_A
                                        when ('0', '0x', '1111', _, '0') => __encoding aarch32_VMAX_f_T1_A // VMAX_f_T1_Q
                                        when ('0', '0x', '1111', _, '1') => __encoding aarch32_VRECPS_T1_A // VRECPS_T1_Q
                                        when (_, _, '0000', _, '0') => __encoding aarch32_VHADD_T1A1_A // VHADD_T1_Q
                                        when ('0', '00', '0001', _, '1') => __encoding aarch32_VAND_r_T1A1_A // VAND_r_T1_Q
                                        when (_, _, '0000', _, '1') => __encoding aarch32_VQADD_T1A1_A // VQADD_T1_Q
                                        when (_, _, '0001', _, '0') => __encoding aarch32_VRHADD_T1A1_A // VRHADD_T1_Q
                                        when ('0', '00', '1100', _, '0') => __encoding aarch32_SHA1C_T1_A // SHA1C_T1
                                        when (_, _, '0010', _, '0') => __encoding aarch32_VHADD_T1A1_A // VHSUB_T1_Q
                                        when ('0', '01', '0001', _, '1') => __encoding aarch32_VBIC_r_T1A1_A // VBIC_r_T1_Q
                                        when (_, _, '0010', _, '1') => __encoding aarch32_VQSUB_T1A1_A // VQSUB_T1_Q
                                        when (_, _, '0011', _, '0') => __encoding aarch32_VCGT_r_T1A1_A
                                        when (_, _, '0011', _, '1') => __encoding aarch32_VCGE_r_T1A1_A
                                        when ('0', '01', '1100', _, '0') => __encoding aarch32_SHA1P_T1_A // SHA1P_T1
                                        when ('0', '1x', '1100', _, '1') => __encoding aarch32_VFMA_T1_A // VFMS_T1_Q
                                        when ('0', '1x', '1101', _, '0') => __encoding aarch32_VSUB_f_T1_A // VSUB_f_T1_Q
                                        when ('0', '1x', '1101', _, '1') => __encoding aarch32_VMLA_f_T1_A // VMLS_f_T1_Q
                                        when ('0', '1x', '1110', _, '0') => __UNALLOCATED
                                        when ('0', '1x', '1111', _, '0') => __encoding aarch32_VMAX_f_T1_A // VMIN_f_T1_Q
                                        when ('0', '1x', '1111', _, '1') => __encoding aarch32_VRSQRTS_T1_A // VRSQRTS_T1_Q
                                        when (_, _, '0100', _, '0') => __encoding aarch32_VSHL_r_T1A1_A // VSHL_r_T1_Q
                                        when ('0', _, '1000', _, '0') => __encoding aarch32_VADD_i_T1A1_A // VADD_i_T1_Q
                                        when ('0', '10', '0001', _, '1') => __encoding aarch32_VORR_r_T1A1_A // VORR_r_T1_Q
                                        when ('0', _, '1000', _, '1') => __encoding aarch32_VTST_T1A1_A // VTST_T1_Q
                                        when (_, _, '0100', _, '1') => __encoding aarch32_VQSHL_r_T1A1_A // VQSHL_r_T1_Q
                                        when ('0', _, '1001', _, '0') => __encoding aarch32_VMLA_i_T1A1_A // VMLA_i_T1_Q
                                        when (_, _, '0101', _, '0') => __encoding aarch32_VRSHL_T1A1_A // VRSHL_T1_Q
                                        when (_, _, '0101', _, '1') => __encoding aarch32_VQRSHL_T1A1_A // VQRSHL_T1_Q
                                        when ('0', _, '1011', _, '0') => __encoding aarch32_VQDMULH_T1A1_A // VQDMULH_T1_Q
                                        when ('0', '10', '1100', _, '0') => __encoding aarch32_SHA1M_T1_A // SHA1M_T1
                                        when ('0', _, '1011', _, '1') => __encoding aarch32_VPADD_i_T1A1_A // VPADD_i_T1
                                        when (_, _, '0110', _, '0') => __encoding aarch32_VMAX_i_T1A1_A // VMAX_i_T1_Q
                                        when ('0', '11', '0001', _, '1') => __encoding aarch32_VORN_r_T1A1_A // VORN_r_T1_Q
                                        when (_, _, '0110', _, '1') => __encoding aarch32_VMAX_i_T1A1_A // VMIN_i_T1_Q
                                        when (_, _, '0111', _, '0') => __encoding aarch32_VABD_i_T1A1_A // VABD_i_T1_Q
                                        when (_, _, '0111', _, '1') => __encoding aarch32_VABA_T1A1_A // VABA_T1_Q
                                        when ('0', '11', '1100', _, '0') => __encoding aarch32_SHA1SU0_T1_A // SHA1SU0_T1
                                        when ('1', '0x', '1101', _, '0') => __encoding aarch32_VPADD_f_T1_A // VPADD_f_T1
                                        when ('1', '0x', '1101', _, '1') => __encoding aarch32_VMUL_f_T1_A // VMUL_f_T1_Q
                                        when ('1', '0x', '1110', _, '0') => __encoding aarch32_VCGE_r_T1A1_A
                                        when ('1', '0x', '1110', _, '1') => __encoding aarch32_VACGE_T1_A // VACGE_T1_Q
                                        when ('1', '0x', '1111', '0', '0') => __encoding aarch32_VPMAX_f_T1_A // VPMAX_f_T1
                                        when ('1', '0x', '1111', _, '1') => __encoding aarch32_VMAXNM_T1_A // VMAXNM_T1_Q
                                        when ('1', '00', '0001', _, '1') => __encoding aarch32_VEOR_T1A1_A // VEOR_T1_Q
                                        when (_, _, '1001', _, '1') => __encoding aarch32_VMUL_i_T1A1_A // VMUL_i_T1_Q
                                        when ('1', '00', '1100', _, '0') => __encoding aarch32_SHA256H_T1_A // SHA256H_T1
                                        when (_, _, '1010', '0', '0') => __encoding aarch32_VPMAX_i_T1A1_A // VPMAX_i_T1
                                        when ('1', '01', '0001', _, '1') => __encoding aarch32_VBIF_T1A1_A // VBSL_T1_Q
                                        when (_, _, '1010', '0', '1') => __encoding aarch32_VPMAX_i_T1A1_A // VPMIN_i_T1
                                        when (_, _, '1010', '1', _) => __UNALLOCATED
                                        when ('1', '01', '1100', _, '0') => __encoding aarch32_SHA256H2_T1_A // SHA256H2_T1
                                        when ('1', '1x', '1101', _, '0') => __encoding aarch32_VABD_f_T1_A // VABD_f_T1_Q
                                        when ('1', '1x', '1110', _, '0') => __encoding aarch32_VCGT_r_T1A1_A
                                        when ('1', '1x', '1110', _, '1') => __encoding aarch32_VACGE_T1_A // VACGT_T1_Q
                                        when ('1', '1x', '1111', '0', '0') => __encoding aarch32_VPMAX_f_T1_A // VPMIN_f_T1
                                        when ('1', '1x', '1111', _, '1') => __encoding aarch32_VMAXNM_T1_A // VMINNM_T1_Q
                                        when ('1', _, '1000', _, '0') => __encoding aarch32_VSUB_i_T1A1_A // VSUB_i_T1_Q
                                        when ('1', '10', '0001', _, '1') => __encoding aarch32_VBIF_T1A1_A // VBIT_T1_Q
                                        when ('1', _, '1000', _, '1') => __encoding aarch32_VCEQ_r_T1A1_A
                                        when ('1', _, '1001', _, '0') => __encoding aarch32_VMLA_i_T1A1_A // VMLS_i_T1_Q
                                        when ('1', _, '1011', _, '0') => __encoding aarch32_VQRDMULH_T1A1_A // VQRDMULH_T1_Q
                                        when ('1', '10', '1100', _, '0') => __encoding aarch32_SHA256SU1_T1_A // SHA256SU1_T1
                                        when ('1', _, '1011', _, '1') => __encoding aarch32_VQRDMLAH_T1_A // VQRDMLAH_T1_Q
                                        when ('1', '11', '0001', _, '1') => __encoding aarch32_VBIF_T1A1_A // VBIF_T1_Q
                                        when ('1', _, '1100', _, '1') => __encoding aarch32_VQRDMLSH_T1_A // VQRDMLSH_T1_Q
                                        when ('1', _, '1111', '1', '0') => __UNALLOCATED
                                when (_, _, _, '1', _, '0', _) =>
                                    // t_simd_mulreg
                                    case (29 +: 3, 28 +: 1, 23 +: 5, 22 +: 1, 20 +: 2, 12 +: 8, 10 +: 2, 7 +: 3, 6 +: 1, 5 +: 1, 4 +: 1, 0 +: 4) of
                                        when (_, '0', _, _, '11', _, _, _, _, _, _, _) => // simd_ext
                                            __field D 22 +: 1
                                            __field Vn 16 +: 4
                                            __field Vd 12 +: 4
                                            __field imm4 8 +: 4
                                            __field N 7 +: 1
                                            __field Q 6 +: 1
                                            __field M 5 +: 1
                                            __field Vm 0 +: 4
                                            case () of
                                                when () => __encoding aarch32_VEXT_T1A1_A // VEXT_T1_Q
                                        when (_, '1', _, _, '11', _, '0x', _, _, _, _, _) => // simd_2r_misc
                                            __field D 22 +: 1
                                            __field size 18 +: 2
                                            __field opc1 16 +: 2
                                            __field Vd 12 +: 4
                                            __field opc2 7 +: 4
                                            __field Q 6 +: 1
                                            __field M 5 +: 1
                                            __field Vm 0 +: 4
                                            case (size, opc1, opc2, Q) of
                                                when (_, '00', '0000', _) => __encoding aarch32_VREV16_T1A1_A // VREV64_T1_Q
                                                when (_, '00', '0001', _) => __encoding aarch32_VREV16_T1A1_A // VREV32_T1_Q
                                                when (_, '00', '0010', _) => __encoding aarch32_VREV16_T1A1_A // VREV16_T1_Q
                                                when (_, '00', '0011', _) => __UNALLOCATED
                                                when (_, '00', '010x', _) => __encoding aarch32_VPADDL_T1A1_A // VPADDL_T1_Q
                                                when (_, '00', '0110', '0') => __encoding aarch32_AESE_T1_A // AESE_T1
                                                when (_, '00', '0110', '1') => __encoding aarch32_AESD_T1_A // AESD_T1
                                                when (_, '00', '0111', '0') => __encoding aarch32_AESMC_T1_A // AESMC_T1
                                                when (_, '00', '0111', '1') => __encoding aarch32_AESIMC_T1_A // AESIMC_T1
                                                when (_, '00', '1000', _) => __encoding aarch32_VCLS_T1A1_A // VCLS_T1_Q
                                                when ('00', '10', '0000', _) => __encoding aarch32_VSWP_T1A1_A // VSWP_T1_Q
                                                when (_, '00', '1001', _) => __encoding aarch32_VCLZ_T1A1_A // VCLZ_T1_Q
                                                when (_, '00', '1010', _) => __encoding aarch32_VCNT_T1A1_A // VCNT_T1_Q
                                                when (_, '00', '1011', _) => __encoding aarch32_VMVN_r_T1A1_A // VMVN_r_T1_Q
                                                when (_, '00', '110x', _) => __encoding aarch32_VPADAL_T1A1_A // VPADAL_T1_Q
                                                when (_, '00', '1110', _) => __encoding aarch32_VQABS_T1A1_A // VQABS_T1_Q
                                                when (_, '00', '1111', _) => __encoding aarch32_VQNEG_T1A1_A // VQNEG_T1_Q
                                                when (_, '01', 'x000', _) => __encoding aarch32_VCGT_i_T1_A // VCGT_i_T1_Q
                                                when (_, '01', 'x001', _) => __encoding aarch32_VCGE_i_T1_A // VCGE_i_T1_Q
                                                when (_, '01', 'x010', _) => __encoding aarch32_VCEQ_i_T1_A // VCEQ_i_T1_Q
                                                when (_, '01', 'x011', _) => __encoding aarch32_VCLE_i_T1_A // VCLE_i_T1_Q
                                                when (_, '01', 'x100', _) => __encoding aarch32_VCLT_i_T1_A // VCLT_i_T1_Q
                                                when (_, '01', 'x110', _) => __encoding aarch32_VABS_T1_A // VABS_T1_Q
                                                when (_, '01', 'x111', _) => __encoding aarch32_VNEG_T1_A // VNEG_T1_Q
                                                when (_, '01', '0101', '1') => __encoding aarch32_SHA1H_T1_A // SHA1H_T1
                                                when (_, '10', '0001', _) => __encoding aarch32_VTRN_T1A1_A // VTRN_T1_Q
                                                when (_, '10', '0010', _) => __encoding aarch32_VUZP_T1A1_A // VUZP_T1_Q
                                                when (_, '10', '0011', _) => __encoding aarch32_VZIP_T1A1_A // VZIP_T1_Q
                                                when (_, '10', '0100', '0') => __encoding aarch32_VMOVN_T1A1_A // VMOVN_T1
                                                when (_, '10', '0100', '1') => __encoding aarch32_VQMOVN_T1A1_A // VQMOVUN_T1
                                                when (_, '10', '0101', _) => __encoding aarch32_VQMOVN_T1A1_A // VQMOVN_T1
                                                when (_, '10', '0110', '0') => __encoding aarch32_VSHLL_T2A2_A // VSHLL_T2
                                                when (_, '10', '0111', '0') => __encoding aarch32_SHA1SU1_T1_A // SHA1SU1_T1
                                                when (_, '10', '0111', '1') => __encoding aarch32_SHA256SU0_T1_A // SHA256SU0_T1
                                                when (_, '10', '1000', _) => __encoding aarch32_VRINTA_asimd_T1_A // VRINTN_asimd_T1_Q
                                                when (_, '10', '1001', _) => __encoding aarch32_VRINTX_asimd_T1_A // VRINTX_asimd_T1_Q
                                                when (_, '10', '1010', _) => __encoding aarch32_VRINTA_asimd_T1_A // VRINTA_asimd_T1_Q
                                                when (_, '10', '1011', _) => __encoding aarch32_VRINTZ_asimd_T1_A // VRINTZ_asimd_T1_Q
                                                when (_, '10', '1100', '0') => __encoding aarch32_VCVT_hs_T1A1_A // VCVT_hs_T1
                                                when (_, '10', '1100', '1') => __UNALLOCATED
                                                when (_, '10', '1101', _) => __encoding aarch32_VRINTA_asimd_T1_A // VRINTM_asimd_T1_Q
                                                when (_, '10', '1110', '0') => __encoding aarch32_VCVT_hs_T1A1_A // VCVT_sh_T1
                                                when (_, '10', '1110', '1') => __UNALLOCATED
                                                when (_, '10', '1111', _) => __encoding aarch32_VRINTA_asimd_T1_A // VRINTP_asimd_T1_Q
                                                when (_, '11', '000x', _) => __encoding aarch32_VCVTA_asimd_T1_A // VCVTA_asimd_T1_Q
                                                when (_, '11', '001x', _) => __encoding aarch32_VCVTA_asimd_T1_A // VCVTN_asimd_T1_Q
                                                when (_, '11', '010x', _) => __encoding aarch32_VCVTA_asimd_T1_A // VCVTP_asimd_T1_Q
                                                when (_, '11', '011x', _) => __encoding aarch32_VCVTA_asimd_T1_A // VCVTM_asimd_T1_Q
                                                when (_, '11', '10x0', _) => __encoding aarch32_VRECPE_T1_A // VRECPE_T1_Q
                                                when (_, '11', '10x1', _) => __encoding aarch32_VRSQRTE_T1_A // VRSQRTE_T1_Q
                                                when (_, '11', '11xx', _) => __encoding aarch32_VCVT_is_T1_A // VCVT_is_T1_Q
                                        when (_, '1', _, _, '11', _, '10', _, _, _, _, _) => // simd_tbl
                                            __field D 22 +: 1
                                            __field Vn 16 +: 4
                                            __field Vd 12 +: 4
                                            __field len 8 +: 2
                                            __field N 7 +: 1
                                            __field op 6 +: 1
                                            __field M 5 +: 1
                                            __field Vm 0 +: 4
                                            case () of
                                                when () => __encoding aarch32_VTBL_T1A1_A // VTBX_T1
                                        when (_, '1', _, _, '11', _, '11', _, _, _, _, _) => // simd_dup_sc
                                            __field D 22 +: 1
                                            __field imm4 16 +: 4
                                            __field Vd 12 +: 4
                                            __field opc 7 +: 3
                                            __field Q 6 +: 1
                                            __field M 5 +: 1
                                            __field Vm 0 +: 4
                                            case (opc) of
                                                when ('000') => __encoding aarch32_VDUP_s_T1A1_A // VDUP_s_T1_Q
                                                when ('001') => __UNALLOCATED
                                                when ('01x') => __UNALLOCATED
                                                when ('1xx') => __UNALLOCATED
                                        when (_, _, _, _, !'11', _, _, _, '0', _, _, _) => // simd_3diff
                                            __field U 28 +: 1
                                            __field D 22 +: 1
                                            __field size 20 +: 2
                                            __field Vn 16 +: 4
                                            __field Vd 12 +: 4
                                            __field opc 8 +: 4
                                            __field N 7 +: 1
                                            __field M 5 +: 1
                                            __field Vm 0 +: 4
                                            case (U, opc) of
                                                when (_, '0000') => __encoding aarch32_VADDL_T1A1_A // VADDL_T1
                                                when (_, '0001') => __encoding aarch32_VADDL_T1A1_A // VADDW_T1
                                                when (_, '0010') => __encoding aarch32_VSUBL_T1A1_A // VSUBL_T1
                                                when ('0', '0100') => __encoding aarch32_VADDHN_T1A1_A // VADDHN_T1
                                                when (_, '0011') => __encoding aarch32_VSUBL_T1A1_A // VSUBW_T1
                                                when ('0', '0110') => __encoding aarch32_VSUBHN_T1A1_A // VSUBHN_T1
                                                when ('0', '1001') => __encoding aarch32_VQDMLAL_T1A1_A // VQDMLAL_T1
                                                when (_, '0101') => __encoding aarch32_VABA_T2A2_A // VABAL_T1
                                                when ('0', '1011') => __encoding aarch32_VQDMLAL_T1A1_A // VQDMLSL_T1
                                                when ('0', '1101') => __encoding aarch32_VQDMULL_T1A1_A // VQDMULL_T1
                                                when (_, '0111') => __encoding aarch32_VABD_i_T2A2_A // VABDL_i_T1
                                                when (_, '1000') => __encoding aarch32_VMLA_i_T2A2_A // VMLAL_i_T1
                                                when (_, '1010') => __encoding aarch32_VMLA_i_T2A2_A // VMLSL_i_T1
                                                when ('1', '0100') => __encoding aarch32_VRADDHN_T1A1_A // VRADDHN_T1
                                                when ('1', '0110') => __encoding aarch32_VRSUBHN_T1A1_A // VRSUBHN_T1
                                                when (_, '11x0') => __encoding aarch32_VMUL_i_T2_A // VMULL_i_T1
                                                when ('1', '1001') => __UNALLOCATED
                                                when ('1', '1011') => __UNALLOCATED
                                                when ('1', '1101') => __UNALLOCATED
                                                when (_, '1111') => __UNALLOCATED
                                        when (_, _, _, _, !'11', _, _, _, '1', _, _, _) => // simd_2r_sc
                                            __field Q 28 +: 1
                                            __field D 22 +: 1
                                            __field size 20 +: 2
                                            __field Vn 16 +: 4
                                            __field Vd 12 +: 4
                                            __field opc 8 +: 4
                                            __field N 7 +: 1
                                            __field M 5 +: 1
                                            __field Vm 0 +: 4
                                            case (Q, opc) of
                                                when (_, '000x') => __encoding aarch32_VMLA_s_T1_A // VMLA_s_T1_Q
                                                when ('0', '0011') => __encoding aarch32_VQDMLAL_T2A2_A // VQDMLAL_T2
                                                when (_, '0010') => __encoding aarch32_VMLA_s_T2A2_A // VMLAL_s_T1
                                                when ('0', '0111') => __encoding aarch32_VQDMLAL_T2A2_A // VQDMLSL_T2
                                                when (_, '010x') => __encoding aarch32_VMLA_s_T1_A // VMLS_s_T1_Q
                                                when ('0', '1011') => __encoding aarch32_VQDMULL_T2A2_A // VQDMULL_T2
                                                when (_, '0110') => __encoding aarch32_VMLA_s_T2A2_A // VMLSL_s_T1
                                                when (_, '100x') => __encoding aarch32_VMUL_s_T1_A // VMUL_s_T1_Q
                                                when ('1', '0011') => __UNALLOCATED
                                                when (_, '1010') => __encoding aarch32_VMUL_s_T2A2_A // VMULL_s_T1
                                                when ('1', '0111') => __UNALLOCATED
                                                when (_, '1100') => __encoding aarch32_VQDMULH_T2A2_A // VQDMULH_T2_Q
                                                when (_, '1101') => __encoding aarch32_VQRDMULH_T2A2_A // VQRDMULH_T2_Q
                                                when ('1', '1011') => __UNALLOCATED
                                                when (_, '1110') => __encoding aarch32_VQRDMLAH_T2_A // VQRDMLAH_T2_Q
                                                when (_, '1111') => __encoding aarch32_VQRDMLSH_T2_A // VQRDMLSH_T2_Q
                                when (_, _, _, '1', _, '1', _) =>
                                    // t_simd_12reg
                                    case (29 +: 3, 28 +: 1, 23 +: 5, 22 +: 1, 7 +: 15, 5 +: 2, 4 +: 1, 0 +: 4) of
                                        when (_, _, _, _, '000xxxxxxxxxxx0', _, _, _) => // simd_1r_imm
                                            __field i 28 +: 1
                                            __field D 22 +: 1
                                            __field imm3 16 +: 3
                                            __field Vd 12 +: 4
                                            __field cmode 8 +: 4
                                            __field Q 6 +: 1
                                            __field op 5 +: 1
                                            __field imm4 0 +: 4
                                            case (cmode, op) of
                                                when ('0xx0', '0') => __encoding aarch32_VMOV_i_T1A1_A
                                                when ('0xx0', '1') => __encoding aarch32_VMVN_i_T1A1_A
                                                when ('0xx1', '0') => __encoding aarch32_VORR_i_T1A1_A
                                                when ('0xx1', '1') => __encoding aarch32_VBIC_i_T1A1_A
                                                when ('10x0', '0') => __encoding aarch32_VMOV_i_T1A1_A
                                                when ('10x0', '1') => __encoding aarch32_VMVN_i_T1A1_A
                                                when ('10x1', '0') => __encoding aarch32_VORR_i_T1A1_A
                                                when ('10x1', '1') => __encoding aarch32_VBIC_i_T1A1_A
                                                when ('11xx', '0') => __encoding aarch32_VMOV_i_T1A1_A
                                                when ('110x', '1') => __encoding aarch32_VMVN_i_T1A1_A
                                                when ('1110', '1') => __encoding aarch32_VMOV_i_T1A1_A
                                                when ('1111', '1') => __UNALLOCATED
                                        when (_, _, _, _, !'000xxxxxxxxxxx0', _, _, _) => // simd_2r_shift
                                            __field U 28 +: 1
                                            __field D 22 +: 1
                                            __field imm3H 19 +: 3
                                            __field imm3L 16 +: 3
                                            __field Vd 12 +: 4
                                            __field opc 8 +: 4
                                            __field L 7 +: 1
                                            __field Q 6 +: 1
                                            __field M 5 +: 1
                                            __field Vm 0 +: 4
                                            case (U, imm3H:L, imm3L, opc, Q) of
                                                when (_, !'0000', _, '0000', _) => __encoding aarch32_VSHR_T1A1_A // VSHR_T1_Q
                                                when (_, !'0000', _, '0001', _) => __encoding aarch32_VSRA_T1A1_A // VSRA_T1_Q
                                                when (_, !'0000', '000', '1010', '0') => __encoding aarch32_VMOVL_T1A1_A // VMOVL_T1
                                                when (_, !'0000', _, '0010', _) => __encoding aarch32_VRSHR_T1A1_A // VRSHR_T1_Q
                                                when (_, !'0000', _, '0011', _) => __encoding aarch32_VRSRA_T1A1_A // VRSRA_T1_Q
                                                when (_, !'0000', _, '0111', _) => __encoding aarch32_VQSHL_i_T1A1_A // VQSHL_i_T1_Q
                                                when (_, !'0000', _, '1001', '0') => __encoding aarch32_VQSHRN_T1A1_A // VQSHRN_T1
                                                when (_, !'0000', _, '1001', '1') => __encoding aarch32_VQRSHRN_T1A1_A // VQRSHRN_T1
                                                when (_, !'0000', _, '1010', '0') => __encoding aarch32_VSHLL_T1A1_A // VSHLL_T1
                                                when (_, !'0000', _, '11xx', _) => __encoding aarch32_VCVT_xs_T1_A // VCVT_xs_T1_Q
                                                when ('0', !'0000', _, '0101', _) => __encoding aarch32_VSHL_i_T1A1_A // VSHL_i_T1_Q
                                                when ('0', !'0000', _, '1000', '0') => __encoding aarch32_VSHRN_T1A1_A // VSHRN_T1
                                                when ('0', !'0000', _, '1000', '1') => __encoding aarch32_VRSHRN_T1A1_A // VRSHRN_T1
                                                when ('1', !'0000', _, '0100', _) => __encoding aarch32_VSRI_T1A1_A // VSRI_T1_Q
                                                when ('1', !'0000', _, '0101', _) => __encoding aarch32_VSLI_T1A1_A // VSLI_T1_Q
                                                when ('1', !'0000', _, '0110', _) => __encoding aarch32_VQSHL_i_T1A1_A // VQSHLU_i_T1_Q
                                                when ('1', !'0000', _, '1000', '0') => __encoding aarch32_VQSHRN_T1A1_A // VQSHRUN_T1
                                                when ('1', !'0000', _, '1000', '1') => __encoding aarch32_VQRSHRN_T1A1_A // VQRSHRUN_T1
                        when (_, '0', _, '0x', _, '10x', _, _, _) =>
                            // simdldst_mov64
                            case (25 +: 7, 21 +: 4, 12 +: 9, 10 +: 2, 0 +: 10) of
                                when (_, '00x0', _, _, _) => // simdfp_mov64
                                    __field D 22 +: 1
                                    __field op 20 +: 1
                                    __field Rt2 16 +: 4
                                    __field Rt 12 +: 4
                                    __field size 8 +: 2
                                    __field opc2 6 +: 2
                                    __field M 5 +: 1
                                    __field o3 4 +: 1
                                    __field Vm 0 +: 4
                                    case (D, op, size, opc2, o3) of
                                        when ('0', _, _, _, _) => __UNALLOCATED
                                        when ('1', _, _, _, '0') => __UNALLOCATED
                                        when ('1', _, '0x', '00', '1') => __UNALLOCATED
                                        when ('1', _, _, '01', _) => __UNALLOCATED
                                        when ('1', '0', '10', '00', '1') => __encoding aarch32_VMOV_ss_T1A1_A // VMOV_toss_T1
                                        when ('1', '0', '11', '00', '1') => __encoding aarch32_VMOV_d_T1A1_A // VMOV_tod_T1
                                        when ('1', _, _, '1x', _) => __UNALLOCATED
                                        when ('1', '1', '10', '00', '1') => __encoding aarch32_VMOV_ss_T1A1_A // VMOV_ss_T1
                                        when ('1', '1', '11', '00', '1') => __encoding aarch32_VMOV_d_T1A1_A // VMOV_d_T1
                                when (_, !'00x0', _, _, _) => // simdfp_ldst
                                    __field P 24 +: 1
                                    __field U 23 +: 1
                                    __field D 22 +: 1
                                    __field W 21 +: 1
                                    __field L 20 +: 1
                                    __field Rn 16 +: 4
                                    __field Vd 12 +: 4
                                    __field size 8 +: 2
                                    __field imm8 0 +: 8
                                    case (P, U, W, L, Rn, size, imm8) of
                                        when ('0', '0', '1', _, _, _, _) => __UNALLOCATED
                                        when ('0', '1', _, _, _, '0x', _) => __UNALLOCATED
                                        when ('0', '1', _, '0', _, '10', _) => __encoding aarch32_VSTM_T2A2_A // VSTM_T2
                                        when ('0', '1', _, '0', _, '11', 'xxxxxxx0') => __encoding aarch32_VSTM_T1A1_A // VSTM_T1
                                        when ('0', '1', _, '0', _, '11', 'xxxxxxx1') => __encoding aarch32_VSTM_T1A1_A // FSTMIAX_T1
                                        when ('0', '1', _, '1', _, '10', _) => __encoding aarch32_VLDM_T2A2_A // VLDM_T2
                                        when ('0', '1', _, '1', _, '11', 'xxxxxxx0') => __encoding aarch32_VLDM_T1A1_A // VLDM_T1
                                        when ('0', '1', _, '1', _, '11', 'xxxxxxx1') => __encoding aarch32_VLDM_T1A1_A // FLDMIAX_T1
                                        when ('1', _, '0', '0', _, _, _) => __encoding aarch32_VSTR_T1_A // VSTR_T1_D
                                        when ('1', _, '0', _, _, '00', _) => __UNALLOCATED
                                        when ('1', _, '0', '1', !'1111', _, _) => __encoding aarch32_VLDR_T1_A // VLDR_T1_D
                                        when ('1', '0', '1', _, _, '0x', _) => __UNALLOCATED
                                        when ('1', '0', '1', '0', _, '10', _) => __encoding aarch32_VSTM_T2A2_A // VSTMDB_T2
                                        when ('1', '0', '1', '0', _, '11', 'xxxxxxx0') => __encoding aarch32_VSTM_T1A1_A // VSTMDB_T1
                                        when ('1', '0', '1', '0', _, '11', 'xxxxxxx1') => __encoding aarch32_VSTM_T1A1_A // FSTMDBX_T1
                                        when ('1', '0', '1', '1', _, '10', _) => __encoding aarch32_VLDM_T2A2_A // VLDMDB_T2
                                        when ('1', '0', '1', '1', _, '11', 'xxxxxxx0') => __encoding aarch32_VLDM_T1A1_A // VLDMDB_T1
                                        when ('1', '0', '1', '1', _, '11', 'xxxxxxx1') => __encoding aarch32_VLDM_T1A1_A // FLDMDBX_T1
                                        when ('1', _, '0', '1', '1111', _, _) => __encoding aarch32_VLDR_T1_A // VLDR_l_T1_D
                                        when ('1', '1', '1', _, _, _, _) => __UNALLOCATED
                        when (_, '0', _, '10', _, '10x', _, '1', _) =>
                            // fpsimd_mov32
                            case (24 +: 8, 21 +: 3, 12 +: 9, 9 +: 3, 8 +: 1, 5 +: 3, 0 +: 5) of
                                when (_, '000', _, _, '0', _, _) => // fp_mov32
                                    __field op 20 +: 1
                                    __field Vn 16 +: 4
                                    __field Rt 12 +: 4
                                    __field N 7 +: 1
                                    case () of
                                        when () => __encoding aarch32_VMOV_s_T1_A // VMOV_s_T1
                                when (_, '111', _, _, '0', _, _) => // fp_msr
                                    __field L 20 +: 1
                                    __field reg 16 +: 4
                                    __field Rt 12 +: 4
                                    case (L) of
                                        when ('0') => __encoding aarch32_VMSR_T1A1_AS // VMSR_T1_AS
                                        when ('1') => __encoding aarch32_VMRS_T1A1_AS // VMRS_T1_AS
                                when (_, _, _, _, '1', _, _) => // simd_dup_el
                                    __field opc1 21 +: 3
                                    __field L 20 +: 1
                                    __field Vn 16 +: 4
                                    __field Rt 12 +: 4
                                    __field N 7 +: 1
                                    __field opc2 5 +: 2
                                    case (opc1, L, opc2) of
                                        when ('0xx', '0', _) => __encoding aarch32_VMOV_rs_T1A1_A // VMOV_rs_T1
                                        when (_, '1', _) => __encoding aarch32_VMOV_sr_T1A1_A // VMOV_sr_T1
                                        when ('1xx', '0', '0x') => __encoding aarch32_VDUP_r_T1A1_A // VDUP_r_T1
                                        when ('1xx', '0', '1x') => __UNALLOCATED
                        when (_, '1', _, '0x', _, '1x0', _, _, _) => // simd_3sameext
                            __field op1 23 +: 2
                            __field D 22 +: 1
                            __field op2 20 +: 2
                            __field Vn 16 +: 4
                            __field Vd 12 +: 4
                            __field op3 10 +: 1
                            __field op4 8 +: 1
                            __field N 7 +: 1
                            __field Q 6 +: 1
                            __field M 5 +: 1
                            __field U 4 +: 1
                            __field Vm 0 +: 4
                            case (op1, op2, op3, op4, Q, U) of
                                when ('x1', '0x', '0', '0', _, '0') => __encoding aarch32_VCADD_T1_A // VCADD_T1_Q
                                when ('00', '10', '0', '0', _, '1') => __encoding aarch32_VFMAL_T1_A // VFMAL_T1_Q
                                when ('00', '10', '1', '1', '0', '0') => __encoding aarch32_VDOT_T1_A // VSDOT_T1_D
                                when ('00', '10', '1', '1', '0', '1') => __encoding aarch32_VDOT_T1_A // VUDOT_T1_D
                                when ('00', '10', '1', '1', '1', '0') => __encoding aarch32_VDOT_T1_A // VSDOT_T1_Q
                                when ('00', '10', '1', '1', '1', '1') => __encoding aarch32_VDOT_T1_A // VUDOT_T1_Q
                                when ('01', '10', '0', '0', _, '1') => __encoding aarch32_VFMAL_T1_A // VFMSL_T1_Q
                                when (_, '1x', '0', '0', _, '0') => __encoding aarch32_VCMLA_T1_A // VCMLA_T1_Q
                        when (_, '1', _, '10', _, '1x0', _, _, _) => // simd_2r_scext
                            __field op1 23 +: 1
                            __field D 22 +: 1
                            __field op2 20 +: 2
                            __field Vn 16 +: 4
                            __field Vd 12 +: 4
                            __field op3 10 +: 1
                            __field op4 8 +: 1
                            __field N 7 +: 1
                            __field Q 6 +: 1
                            __field M 5 +: 1
                            __field U 4 +: 1
                            __field Vm 0 +: 4
                            case (op1, op2, op3, op4, Q, U) of
                                when ('0', _, '0', '0', _, '0') => __encoding aarch32_VCMLA_idx_T1_A // VCMLA_s_T1_QH
                                when ('0', '00', '0', '0', _, '1') => __encoding aarch32_VFMAL_i_T1_A // VFMAL_s_T1_Q
                                when ('0', '01', '0', '0', _, '1') => __encoding aarch32_VFMAL_i_T1_A // VFMSL_s_T1_Q
                                when ('0', '10', '1', '1', '0', '0') => __encoding aarch32_VDOT_s_T1_A // VSDOT_s_T1_D
                                when ('0', '10', '1', '1', '0', '1') => __encoding aarch32_VDOT_s_T1_A // VUDOT_s_T1_D
                                when ('0', '10', '1', '1', '1', '0') => __encoding aarch32_VDOT_s_T1_A // VSDOT_s_T1_Q
                                when ('0', '10', '1', '1', '1', '1') => __encoding aarch32_VDOT_s_T1_A // VUDOT_s_T1_Q
                                when ('1', _, '0', '0', _, '0') => __encoding aarch32_VCMLA_idx_T1_A // VCMLA_s_T1_QS
                when (_, '0100', 'xx0xx', _, _, _) => // ldstm
                    __field opc 23 +: 2
                    __field W 21 +: 1
                    __field L 20 +: 1
                    __field Rn 16 +: 4
                    __field P 15 +: 1
                    __field M 14 +: 1
                    __field register_list 0 +: 14
                    case (opc, L) of
                        when ('00', '0') => __encoding aarch32_SRS_A1_AS
                        when ('00', '1') => __encoding aarch32_RFE_A1_AS
                        when ('01', '0') => __encoding aarch32_STM_T2_A // STM_T2
                        when ('01', '1') => __encoding aarch32_LDM_T2_A // LDM_T2
                        when ('10', '0') => __encoding aarch32_STMDB_T1_A // STMDB_T1
                        when ('10', '1') => __encoding aarch32_LDMDB_T1_A // LDMDB_T1
                        when ('11', '0') => __encoding aarch32_SRS_A1_AS
                        when ('11', '1') => __encoding aarch32_RFE_A1_AS
                when (_, '0100', 'xx1xx', _, _, _) =>
                    // dstd
                    case (25 +: 7, 21 +: 4, 20 +: 1, 16 +: 4, 8 +: 8, 5 +: 3, 0 +: 5) of
                        when (_, '0010', _, _, _, _, _) => // ldstex
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field Rd 8 +: 4
                            __field imm8 0 +: 8
                            case (L) of
                                when ('0') => __encoding aarch32_STREX_T1_A // STREX_T1
                                when ('1') => __encoding aarch32_LDREX_T1_A // LDREX_T1
                        when (_, '0110', '0', _, _, '000', _) => __UNPREDICTABLE
                        when (_, '0110', '1', _, _, '000', _) => // tblbr
                            __field Rn 16 +: 4
                            __field H 4 +: 1
                            __field Rm 0 +: 4
                            case () of
                                when () => __encoding aarch32_TBB_T1_A // TBH_T1
                        when (_, '0110', _, _, _, '01x', _) => // ldstex_bhd
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field Rt2 8 +: 4
                            __field sz 4 +: 2
                            __field Rd 0 +: 4
                            case (L, sz) of
                                when ('0', '00') => __encoding aarch32_STREXB_T1_A // STREXB_T1
                                when ('0', '01') => __encoding aarch32_STREXH_T1_A // STREXH_T1
                                when ('0', '10') => __UNALLOCATED
                                when ('0', '11') => __encoding aarch32_STREXD_T1_A // STREXD_T1
                                when ('1', '00') => __encoding aarch32_LDREXB_T1_A // LDREXB_T1
                                when ('1', '01') => __encoding aarch32_LDREXH_T1_A // LDREXH_T1
                                when ('1', '10') => __UNALLOCATED
                                when ('1', '11') => __encoding aarch32_LDREXD_T1_A // LDREXD_T1
                        when (_, '0110', _, _, _, '1xx', _) => // ldastl
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field Rt2 8 +: 4
                            __field op 6 +: 1
                            __field sz 4 +: 2
                            __field Rd 0 +: 4
                            case (L, op, sz) of
                                when ('0', '0', '00') => __encoding aarch32_STLB_T1_A // STLB_T1
                                when ('0', '0', '01') => __encoding aarch32_STLH_T1_A // STLH_T1
                                when ('0', '0', '10') => __encoding aarch32_STL_T1_A // STL_T1
                                when ('0', '0', '11') => __UNALLOCATED
                                when ('0', '1', '00') => __encoding aarch32_STLEXB_T1_A // STLEXB_T1
                                when ('0', '1', '01') => __encoding aarch32_STLEXH_T1_A // STLEXH_T1
                                when ('0', '1', '10') => __encoding aarch32_STLEX_T1_A // STLEX_T1
                                when ('0', '1', '11') => __encoding aarch32_STLEXD_T1_A // STLEXD_T1
                                when ('1', '0', '00') => __encoding aarch32_LDAB_T1_A // LDAB_T1
                                when ('1', '0', '01') => __encoding aarch32_LDAH_T1_A // LDAH_T1
                                when ('1', '0', '10') => __encoding aarch32_LDA_T1_A // LDA_T1
                                when ('1', '0', '11') => __UNALLOCATED
                                when ('1', '1', '00') => __encoding aarch32_LDAEXB_T1_A // LDAEXB_T1
                                when ('1', '1', '01') => __encoding aarch32_LDAEXH_T1_A // LDAEXH_T1
                                when ('1', '1', '10') => __encoding aarch32_LDAEX_T1_A // LDAEX_T1
                                when ('1', '1', '11') => __encoding aarch32_LDAEXD_T1_A // LDAEXD_T1
                        when (_, '0x11', _, !'1111', _, _, _) => // ldstd_post
                            __field U 23 +: 1
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field Rt2 8 +: 4
                            __field imm8 0 +: 8
                            case (L) of
                                when ('0') => __encoding aarch32_STRD_i_T1_A // STRD_i_T1_post
                                when ('1') => __encoding aarch32_LDRD_i_T1_A // LDRD_i_T1_post
                        when (_, '1x10', _, !'1111', _, _, _) => // ldstd_imm
                            __field U 23 +: 1
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field Rt2 8 +: 4
                            __field imm8 0 +: 8
                            case (L) of
                                when ('0') => __encoding aarch32_STRD_i_T1_A // STRD_i_T1_off
                                when ('1') => __encoding aarch32_LDRD_i_T1_A // LDRD_i_T1_off
                        when (_, '1x11', _, !'1111', _, _, _) => // ldstd_pre
                            __field U 23 +: 1
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field Rt2 8 +: 4
                            __field imm8 0 +: 8
                            case (L) of
                                when ('0') => __encoding aarch32_STRD_i_T1_A // STRD_i_T1_pre
                                when ('1') => __encoding aarch32_LDRD_i_T1_A // LDRD_i_T1_pre
                        when (_, !'0xx0', _, '1111', _, _, _) => // lddlit
                            __field P 24 +: 1
                            __field U 23 +: 1
                            __field W 21 +: 1
                            __field L 20 +: 1
                            __field Rt 12 +: 4
                            __field Rt2 8 +: 4
                            __field imm8 0 +: 8
                            case (L) of
                                when ('1') => __encoding aarch32_LDRD_l_T1_A // LDRD_l_T1
                when (_, '0101', _, _, _, _) => // dpint_shiftr
                    __field op1 21 +: 4
                    __field S 20 +: 1
                    __field Rn 16 +: 4
                    __field imm3 12 +: 3
                    __field Rd 8 +: 4
                    __field imm2 6 +: 2
                    __field type1 4 +: 2
                    __field Rm 0 +: 4
                    case (op1, S, Rn, imm3:imm2:type1, Rd) of
                        when ('0000', '0', _, _, _) => __encoding aarch32_AND_r_T2_A // AND_r_T2_RRX
                        when ('0000', '1', _, !'0000011', !'1111') => __encoding aarch32_AND_r_T2_A // ANDS_r_T2
                        when ('0000', '1', _, !'0000011', '1111') => __encoding aarch32_TST_r_T2_A // TST_r_T2
                        when ('0000', '1', _, '0000011', !'1111') => __encoding aarch32_AND_r_T2_A // ANDS_r_T2_RRX
                        when ('0000', '1', _, '0000011', '1111') => __encoding aarch32_TST_r_T2_A // TST_r_T2_RRX
                        when ('0001', _, _, _, _) => __encoding aarch32_BIC_r_T2_A // BICS_r_T2_RRX
                        when ('0010', '0', !'1111', _, _) => __encoding aarch32_ORR_r_T2_A // ORR_r_T2_RRX
                        when ('0010', '0', '1111', _, _) => __encoding aarch32_MOV_r_T3_A // MOV_r_T3_RRX
                        when ('0010', '1', !'1111', _, _) => __encoding aarch32_ORR_r_T2_A // ORRS_r_T2_RRX
                        when ('0010', '1', '1111', _, _) => __encoding aarch32_MOV_r_T3_A // MOVS_r_T3_RRX
                        when ('0011', '0', !'1111', _, _) => __encoding aarch32_ORN_r_T1_A // ORN_r_T1_RRX
                        when ('0011', '0', '1111', _, _) => __encoding aarch32_MVN_r_T2_A // MVN_r_T2_RRX
                        when ('0011', '1', !'1111', _, _) => __encoding aarch32_ORN_r_T1_A // ORNS_r_T1_RRX
                        when ('0011', '1', '1111', _, _) => __encoding aarch32_MVN_r_T2_A // MVNS_r_T2_RRX
                        when ('0100', '0', _, _, _) => __encoding aarch32_EOR_r_T2_A // EOR_r_T2_RRX
                        when ('0100', '1', _, !'0000011', !'1111') => __encoding aarch32_EOR_r_T2_A // EORS_r_T2
                        when ('0100', '1', _, !'0000011', '1111') => __encoding aarch32_TEQ_r_T1_A // TEQ_r_T1
                        when ('0100', '1', _, '0000011', !'1111') => __encoding aarch32_EOR_r_T2_A // EORS_r_T2_RRX
                        when ('0100', '1', _, '0000011', '1111') => __encoding aarch32_TEQ_r_T1_A // TEQ_r_T1_RRX
                        when ('0101', _, _, _, _) => __UNALLOCATED
                        when ('0110', '0', _, 'xxxxx00', _) => __encoding aarch32_PKH_T1_A // PKHBT_T1
                        when ('0110', '0', _, 'xxxxx01', _) => __UNALLOCATED
                        when ('0110', '0', _, 'xxxxx10', _) => __encoding aarch32_PKH_T1_A // PKHTB_T1
                        when ('0110', '0', _, 'xxxxx11', _) => __UNALLOCATED
                        when ('0111', _, _, _, _) => __UNALLOCATED
                        when ('1000', '0', !'1101', _, _) => __encoding aarch32_ADD_r_T3_A // ADD_r_T3_RRX
                        when ('1000', '0', '1101', _, _) => __encoding aarch32_ADD_SP_r_T3_A // ADD_SP_r_T3_RRX
                        when ('1000', '1', !'1101', _, !'1111') => __encoding aarch32_ADD_r_T3_A // ADDS_r_T3_RRX
                        when ('1000', '1', '1101', _, !'1111') => __encoding aarch32_ADD_SP_r_T3_A // ADDS_SP_r_T3_RRX
                        when ('1000', '1', _, _, '1111') => __encoding aarch32_CMN_r_T2_A // CMN_r_T2_RRX
                        when ('1001', _, _, _, _) => __UNALLOCATED
                        when ('1010', _, _, _, _) => __encoding aarch32_ADC_r_T2_A // ADCS_r_T2_RRX
                        when ('1011', _, _, _, _) => __encoding aarch32_SBC_r_T2_A // SBCS_r_T2_RRX
                        when ('1100', _, _, _, _) => __UNALLOCATED
                        when ('1101', '0', !'1101', _, _) => __encoding aarch32_SUB_r_T2_A // SUB_r_T2_RRX
                        when ('1101', '0', '1101', _, _) => __encoding aarch32_SUB_SP_r_T1_A // SUB_SP_r_T1_RRX
                        when ('1101', '1', !'1101', _, !'1111') => __encoding aarch32_SUB_r_T2_A // SUBS_r_T2_RRX
                        when ('1101', '1', '1101', _, !'1111') => __encoding aarch32_SUB_SP_r_T1_A // SUBS_SP_r_T1_RRX
                        when ('1101', '1', _, _, '1111') => __encoding aarch32_CMP_r_T3_A // CMP_r_T3_RRX
                        when ('1110', _, _, _, _) => __encoding aarch32_RSB_r_T1_A // RSBS_r_T1_RRX
                        when ('1111', _, _, _, _) => __UNALLOCATED
                when (_, '10xx', _, _, '1', _) =>
                    // bcrtrl
                    case (27 +: 5, 26 +: 1, 22 +: 4, 20 +: 2, 16 +: 4, 15 +: 1, 12 +: 3, 11 +: 1, 8 +: 3, 6 +: 2, 5 +: 1, 0 +: 5) of
                        when (_, '0', '1110', '0x', _, _, '0x0', _, _, _, '0', _) => // msr_spec
                            __field R 20 +: 1
                            __field Rn 16 +: 4
                            __field mask 8 +: 4
                            case () of
                                when () => __encoding aarch32_MSR_r_T1_AS // MSR_r_T1_AS
                        when (_, '0', '1110', '0x', _, _, '0x0', _, _, _, '1', _) => // msr_bank
                            __field R 20 +: 1
                            __field Rn 16 +: 4
                            __field m1 8 +: 4
                            __field m 4 +: 1
                            case () of
                                when () => __encoding aarch32_MSR_br_T1_AS // MSR_br_T1_AS
                        when (_, '0', '1110', '10', _, _, '0x0', _, '000', _, _, _) => // hints
                            __field hint 4 +: 4
                            __field option 0 +: 4
                            case (hint, option) of
                                when ('0000', '0000') => __encoding aarch32_NOP_T2_A // NOP_T2
                                when ('0000', '0001') => __encoding aarch32_YIELD_T2_A // YIELD_T2
                                when ('0000', '0010') => __encoding aarch32_WFE_T2_A // WFE_T2
                                when ('0000', '0011') => __encoding aarch32_WFI_T2_A // WFI_T2
                                when ('0000', '0100') => __encoding aarch32_SEV_T2_A // SEV_T2
                                when ('0000', '0101') => __encoding aarch32_SEVL_T2_A // SEVL_T2
                                when ('0000', '011x') => __NOP
                                when ('0000', '1xxx') => __NOP
                                when ('0001', '0000') => __encoding aarch32_ESB_T1_A // ESB_T1
                                when ('0001', '0001') => __NOP
                                when ('0001', '0010') => __encoding aarch32_TSB_T1_A // TSB_T1
                                when ('0001', '0011') => __NOP
                                when ('0001', '0100') => __encoding aarch32_CSDB_T1_A // CSDB_T1
                                when ('0001', '0101') => __NOP
                                when ('0001', '011x') => __NOP
                                when ('0001', '1xxx') => __NOP
                                when ('001x', _) => __NOP
                                when ('01xx', _) => __NOP
                                when ('10xx', _) => __NOP
                                when ('110x', _) => __NOP
                                when ('1110', _) => __NOP
                                when ('1111', _) => __encoding aarch32_DBG_T1_A // DBG_T1
                        when (_, '0', '1110', '10', _, _, '0x0', _, !'000', _, _, _) => // cps
                            __field imod 9 +: 2
                            __field M 8 +: 1
                            __field A 7 +: 1
                            __field I 6 +: 1
                            __field F 5 +: 1
                            __field mode 0 +: 5
                            case (imod, M) of
                                when ('00', '1') => __encoding aarch32_CPS_T2_AS // CPS_T2_AS
                                when ('01', _) => __UNALLOCATED
                                when ('10', _) => __encoding aarch32_CPS_T2_AS // CPSIE_T2_ASM
                                when ('11', _) => __encoding aarch32_CPS_T2_AS // CPSID_T2_ASM
                        when (_, '0', '1110', '11', _, _, '0x0', _, _, _, _, _) => // system
                            __field opc 4 +: 4
                            __field option 0 +: 4
                            case (opc, option) of
                                when ('000x', _) => __UNALLOCATED
                                when ('0010', _) => __encoding aarch32_CLREX_T1_A // CLREX_T1
                                when ('0011', _) => __UNALLOCATED
                                when ('0100', !'0x00') => __encoding aarch32_DSB_T1_A // DSB_T1
                                when ('0100', '0000') => __encoding aarch32_SSBB_T1_A // SSBB_T1
                                when ('0100', '0100') => __encoding aarch32_PSSBB_T1_A // PSSBB_T1
                                when ('0101', _) => __encoding aarch32_DMB_T1_A // DMB_T1
                                when ('0110', _) => __encoding aarch32_ISB_T1_A // ISB_T1
                                when ('0111', _) => __encoding aarch32_SB_T1_A // SB_T1
                                when ('1xxx', _) => __UNALLOCATED
                        when (_, '0', '1111', '00', _, _, '0x0', _, _, _, _, _) => // bx_jaz
                            __field Rm 16 +: 4
                            case () of
                                when () => __encoding aarch32_BXJ_T1_A // BXJ_T1
                        when (_, '0', '1111', '01', _, _, '0x0', _, _, _, _, _) => // eret
                            __field Rn 16 +: 4
                            __field imm8 0 +: 8
                            case (Rn, imm8) of
                                when (_, !'00000000') => __encoding aarch32_SUB_i_T5_AS // SUBS_PC_T5_AS
                                when ('1110', '00000000') => __encoding aarch32_ERET_T1_A // ERET_T1
                        when (_, '0', '1111', '1x', _, _, '0x0', _, _, _, '0', _) => // mrs_spec
                            __field R 20 +: 1
                            __field Rd 8 +: 4
                            case () of
                                when () => __encoding aarch32_MRS_T1_AS // MRS_T1_AS
                        when (_, '0', '1111', '1x', _, _, '0x0', _, _, _, '1', _) => // mrs_bank
                            __field R 20 +: 1
                            __field m1 16 +: 4
                            __field Rd 8 +: 4
                            __field m 4 +: 1
                            case () of
                                when () => __encoding aarch32_MRS_br_T1_AS // MRS_br_T1_AS
                        when (_, '1', '1110', '00', _, _, '000', _, _, _, _, _) => // dcps
                            __field imm4 16 +: 4
                            __field imm10 2 +: 10
                            __field opt 0 +: 2
                            case (imm4, imm10, opt) of
                                when (!'1111', _, _) => __UNALLOCATED
                                when ('1111', !'0000000000', _) => __UNALLOCATED
                                when ('1111', '0000000000', '00') => __UNALLOCATED
                                when ('1111', '0000000000', '01') => __encoding aarch32_DCPS_T1_A // DCPS1_T1
                                when ('1111', '0000000000', '10') => __encoding aarch32_DCPS_T1_A // DCPS2_T1
                                when ('1111', '0000000000', '11') => __encoding aarch32_DCPS_T1_A // DCPS3_T1
                        when (_, '1', '1110', '00', _, _, '010', _, _, _, _, _) => __UNPREDICTABLE
                        when (_, '1', '1110', '01', _, _, '0x0', _, _, _, _, _) => __UNPREDICTABLE
                        when (_, '1', '1110', '1x', _, _, '0x0', _, _, _, _, _) => __UNPREDICTABLE
                        when (_, '1', '1111', '0x', _, _, '0x0', _, _, _, _, _) => __UNPREDICTABLE
                        when (_, '1', '1111', '1x', _, _, '0x0', _, _, _, _, _) => // except
                            __field o1 20 +: 1
                            __field imm4 16 +: 4
                            __field o2 13 +: 1
                            __field imm12 0 +: 12
                            case (o1, o2) of
                                when ('0', '0') => __encoding aarch32_HVC_T1_A // HVC_T1
                                when ('0', '1') => __UNALLOCATED
                                when ('1', '0') => __encoding aarch32_SMC_T1_AS // SMC_T1_AS
                                when ('1', '1') => __encoding aarch32_UDF_T2_A // UDF_T2
                        when (_, _, !'111x', _, _, _, '0x0', _, _, _, _, _) => // bcond
                            __field S 26 +: 1
                            __field cond 22 +: 4
                            __field imm6 16 +: 6
                            __field J1 13 +: 1
                            __field J2 11 +: 1
                            __field imm11 0 +: 11
                            case () of
                                when () => __encoding aarch32_B_T3_A // B_T3
                        when (_, _, _, _, _, _, '0x1', _, _, _, _, _) => // b
                            __field S 26 +: 1
                            __field imm10 16 +: 10
                            __field J1 13 +: 1
                            __field J2 11 +: 1
                            __field imm11 0 +: 11
                            case () of
                                when () => __encoding aarch32_B_T4_A // B_T4
                        when (_, _, _, _, _, _, '1x0', _, _, _, _, _) => // blx
                            __field S 26 +: 1
                            __field imm10 16 +: 10
                            __field J1 13 +: 1
                            __field J2 11 +: 1
                            __field imm11 0 +: 11
                            case () of
                                when () => __encoding aarch32_BL_i_T2_A // BL_i_T2
                        when (_, _, _, _, _, _, '1x1', _, _, _, _, _) => // bl
                            __field S 26 +: 1
                            __field imm10 16 +: 10
                            __field J1 13 +: 1
                            __field J2 11 +: 1
                            __field imm11 0 +: 11
                            case () of
                                when () => __encoding aarch32_BL_i_T1_A // BL_i_T1
                when (_, '10x0', _, _, '0', _) => // dpint_immm
                    __field i 26 +: 1
                    __field op1 21 +: 4
                    __field S 20 +: 1
                    __field Rn 16 +: 4
                    __field imm3 12 +: 3
                    __field Rd 8 +: 4
                    __field imm8 0 +: 8
                    case (op1, S, Rn, Rd) of
                        when ('0000', '0', _, _) => __encoding aarch32_AND_i_T1_A // AND_i_T1
                        when ('0000', '1', _, !'1111') => __encoding aarch32_AND_i_T1_A // ANDS_i_T1
                        when ('0000', '1', _, '1111') => __encoding aarch32_TST_i_T1_A // TST_i_T1
                        when ('0001', _, _, _) => __encoding aarch32_BIC_i_T1_A // BICS_i_T1
                        when ('0010', '0', !'1111', _) => __encoding aarch32_ORR_i_T1_A // ORR_i_T1
                        when ('0010', '0', '1111', _) => __encoding aarch32_MOV_i_T2_A // MOV_i_T2
                        when ('0010', '1', !'1111', _) => __encoding aarch32_ORR_i_T1_A // ORRS_i_T1
                        when ('0010', '1', '1111', _) => __encoding aarch32_MOV_i_T2_A // MOVS_i_T2
                        when ('0011', '0', !'1111', _) => __encoding aarch32_ORN_i_T1_A // ORN_i_T1
                        when ('0011', '0', '1111', _) => __encoding aarch32_MVN_i_T1_A // MVN_i_T1
                        when ('0011', '1', !'1111', _) => __encoding aarch32_ORN_i_T1_A // ORNS_i_T1
                        when ('0011', '1', '1111', _) => __encoding aarch32_MVN_i_T1_A // MVNS_i_T1
                        when ('0100', '0', _, _) => __encoding aarch32_EOR_i_T1_A // EOR_i_T1
                        when ('0100', '1', _, !'1111') => __encoding aarch32_EOR_i_T1_A // EORS_i_T1
                        when ('0100', '1', _, '1111') => __encoding aarch32_TEQ_i_T1_A // TEQ_i_T1
                        when ('0101', _, _, _) => __UNALLOCATED
                        when ('011x', _, _, _) => __UNALLOCATED
                        when ('1000', '0', !'1101', _) => __encoding aarch32_ADD_i_T3_A // ADD_i_T3
                        when ('1000', '0', '1101', _) => __encoding aarch32_ADD_SP_i_T3_A // ADD_SP_i_T3
                        when ('1000', '1', !'1101', !'1111') => __encoding aarch32_ADD_i_T3_A // ADDS_i_T3
                        when ('1000', '1', '1101', !'1111') => __encoding aarch32_ADD_SP_i_T3_A // ADDS_SP_i_T3
                        when ('1000', '1', _, '1111') => __encoding aarch32_CMN_i_T1_A // CMN_i_T1
                        when ('1001', _, _, _) => __UNALLOCATED
                        when ('1010', _, _, _) => __encoding aarch32_ADC_i_T1_A // ADCS_i_T1
                        when ('1011', _, _, _) => __encoding aarch32_SBC_i_T1_A // SBCS_i_T1
                        when ('1100', _, _, _) => __UNALLOCATED
                        when ('1101', '0', !'1101', _) => __encoding aarch32_SUB_i_T3_A // SUB_i_T3
                        when ('1101', '0', '1101', _) => __encoding aarch32_SUB_SP_i_T2_A // SUB_SP_i_T2
                        when ('1101', '1', !'1101', !'1111') => __encoding aarch32_SUB_i_T3_A // SUBS_i_T3
                        when ('1101', '1', '1101', !'1111') => __encoding aarch32_SUB_SP_i_T2_A // SUBS_SP_i_T2
                        when ('1101', '1', _, '1111') => __encoding aarch32_CMP_i_T2_A // CMP_i_T2
                        when ('1110', _, _, _) => __encoding aarch32_RSB_i_T2_A // RSBS_i_T2
                        when ('1111', _, _, _) => __UNALLOCATED
                when (_, '10x1', _, _, '0', _) =>
                    // imm
                    case (27 +: 5, 26 +: 1, 25 +: 1, 24 +: 1, 23 +: 1, 21 +: 2, 20 +: 1, 16 +: 4, 15 +: 1, 0 +: 15) of
                        when (_, _, _, '0', _, '0x', _, _, _, _) => // dpint_imms
                            __field i 26 +: 1
                            __field o1 23 +: 1
                            __field o2 21 +: 1
                            __field Rn 16 +: 4
                            __field imm3 12 +: 3
                            __field Rd 8 +: 4
                            __field imm8 0 +: 8
                            case (o1, o2, Rn) of
                                when ('0', '0', !'11x1') => __encoding aarch32_ADD_i_T4_A // ADD_i_T4
                                when ('0', '0', '1101') => __encoding aarch32_ADD_SP_i_T4_A // ADD_SP_i_T4
                                when ('0', '0', '1111') => __encoding aarch32_ADR_A1_A
                                when ('0', '1', _) => __UNALLOCATED
                                when ('1', '0', _) => __UNALLOCATED
                                when ('1', '1', !'11x1') => __encoding aarch32_SUB_i_T4_A // SUB_i_T4
                                when ('1', '1', '1101') => __encoding aarch32_SUB_SP_i_T3_A // SUB_SP_i_T3
                                when ('1', '1', '1111') => __encoding aarch32_ADR_A1_A
                        when (_, _, _, '0', _, '10', _, _, _, _) => // movw
                            __field i 26 +: 1
                            __field o1 23 +: 1
                            __field imm4 16 +: 4
                            __field imm3 12 +: 3
                            __field Rd 8 +: 4
                            __field imm8 0 +: 8
                            case (o1) of
                                when ('0') => __encoding aarch32_MOV_i_T3_A // MOV_i_T3
                                when ('1') => __encoding aarch32_MOVT_T1_A // MOVT_T1
                        when (_, _, _, '0', _, '11', _, _, _, _) => __UNPREDICTABLE
                        when (_, _, _, '1', _, _, _, _, _, _) => // sat_bit
                            __field op1 21 +: 3
                            __field Rn 16 +: 4
                            __field imm3 12 +: 3
                            __field Rd 8 +: 4
                            __field imm2 6 +: 2
                            __field widthm1 0 +: 5
                            case (op1, Rn, imm3:imm2) of
                                when ('000', _, _) => __encoding aarch32_SSAT_T1_A // SSAT_T1_LSL
                                when ('001', _, !'00000') => __encoding aarch32_SSAT_T1_A // SSAT_T1_ASR
                                when ('001', _, '00000') => __encoding aarch32_SSAT16_T1_A // SSAT16_T1
                                when ('010', _, _) => __encoding aarch32_SBFX_T1_A // SBFX_T1
                                when ('011', !'1111', _) => __encoding aarch32_BFI_T1_A // BFI_T1
                                when ('011', '1111', _) => __encoding aarch32_BFC_T1_A // BFC_T1
                                when ('100', _, _) => __encoding aarch32_USAT_T1_A // USAT_T1_LSL
                                when ('101', _, !'00000') => __encoding aarch32_USAT_T1_A // USAT_T1_ASR
                                when ('101', _, '00000') => __encoding aarch32_USAT16_T1_A // USAT16_T1
                                when ('110', _, _) => __encoding aarch32_UBFX_T1_A // UBFX_T1
                                when ('111', _, _) => __UNALLOCATED
                when (_, '1100', '1xxx0', _, _, _) =>
                    // vldst
                    case (24 +: 8, 23 +: 1, 21 +: 2, 20 +: 1, 12 +: 8, 10 +: 2, 0 +: 10) of
                        when (_, '0', _, _, _, _, _) => // asimldstms
                            __field D 22 +: 1
                            __field L 21 +: 1
                            __field Rn 16 +: 4
                            __field Vd 12 +: 4
                            __field type1 8 +: 4
                            __field size 6 +: 2
                            __field align 4 +: 2
                            __field Rm 0 +: 4
                            case (L, type1) of
                                when ('0', '000x') => __encoding aarch32_VST4_m_T1A1_A // VST4_m_T1_posti
                                when ('0', '0010') => __encoding aarch32_VST1_m_T1A1_A
                                when ('0', '0011') => __encoding aarch32_VST2_m_T1A1_A
                                when ('0', '010x') => __encoding aarch32_VST3_m_T1A1_A // VST3_m_T1_posti
                                when ('0', '0110') => __encoding aarch32_VST1_m_T1A1_A
                                when ('0', '0111') => __encoding aarch32_VST1_m_T1A1_A
                                when ('0', '100x') => __encoding aarch32_VST2_m_T1A1_A
                                when ('0', '1010') => __encoding aarch32_VST1_m_T1A1_A
                                when ('1', '000x') => __encoding aarch32_VLD4_m_T1A1_A // VLD4_m_T1_posti
                                when ('1', '0010') => __encoding aarch32_VLD1_m_T1A1_A
                                when ('1', '0011') => __encoding aarch32_VLD2_m_T1A1_A
                                when ('1', '010x') => __encoding aarch32_VLD3_m_T1A1_A // VLD3_m_T1_posti
                                when (_, '1011') => __UNALLOCATED
                                when ('1', '0110') => __encoding aarch32_VLD1_m_T1A1_A
                                when ('1', '0111') => __encoding aarch32_VLD1_m_T1A1_A
                                when (_, '11xx') => __UNALLOCATED
                                when ('1', '100x') => __encoding aarch32_VLD2_m_T1A1_A
                                when ('1', '1010') => __encoding aarch32_VLD1_m_T1A1_A
                        when (_, '1', _, _, _, '11', _) => // asimldall
                            __field D 22 +: 1
                            __field L 21 +: 1
                            __field Rn 16 +: 4
                            __field Vd 12 +: 4
                            __field N 8 +: 2
                            __field size 6 +: 2
                            __field T 5 +: 1
                            __field a 4 +: 1
                            __field Rm 0 +: 4
                            case (L, N, a) of
                                when ('0', _, _) => __UNALLOCATED
                                when ('1', '00', _) => __encoding aarch32_VLD1_a_T1A1_A // VLD1_a_T1_posti
                                when ('1', '01', _) => __encoding aarch32_VLD2_a_T1A1_A // VLD2_a_T1_posti
                                when ('1', '10', '0') => __encoding aarch32_VLD3_a_T1A1_A // VLD3_a_T1_posti
                                when ('1', '10', '1') => __UNALLOCATED
                                when ('1', '11', _) => __encoding aarch32_VLD4_a_T1A1_A // VLD4_a_T1_posti
                        when (_, '1', _, _, _, !'11', _) => // asimldstss
                            __field D 22 +: 1
                            __field L 21 +: 1
                            __field Rn 16 +: 4
                            __field Vd 12 +: 4
                            __field size 10 +: 2
                            __field N 8 +: 2
                            __field index_align 4 +: 4
                            __field Rm 0 +: 4
                            case (L, size, N) of
                                when ('0', '00', '00') => __encoding aarch32_VST1_1_T1A1_A
                                when ('0', '00', '01') => __encoding aarch32_VST2_1_T1A1_A
                                when ('0', '00', '10') => __encoding aarch32_VST3_1_T1A1_A
                                when ('0', '00', '11') => __encoding aarch32_VST4_1_T1A1_A
                                when ('0', '01', '00') => __encoding aarch32_VST1_1_T1A1_A
                                when ('0', '01', '01') => __encoding aarch32_VST2_1_T1A1_A
                                when ('0', '01', '10') => __encoding aarch32_VST3_1_T1A1_A
                                when ('0', '01', '11') => __encoding aarch32_VST4_1_T1A1_A
                                when ('0', '10', '00') => __encoding aarch32_VST1_1_T1A1_A
                                when ('0', '10', '01') => __encoding aarch32_VST2_1_T1A1_A
                                when ('0', '10', '10') => __encoding aarch32_VST3_1_T1A1_A
                                when ('0', '10', '11') => __encoding aarch32_VST4_1_T1A1_A
                                when ('1', '00', '00') => __encoding aarch32_VLD1_1_T1A1_A
                                when ('1', '00', '01') => __encoding aarch32_VLD2_1_T1A1_A
                                when ('1', '00', '10') => __encoding aarch32_VLD3_1_T1A1_A
                                when ('1', '00', '11') => __encoding aarch32_VLD4_1_T1A1_A
                                when ('1', '01', '00') => __encoding aarch32_VLD1_1_T1A1_A
                                when ('1', '01', '01') => __encoding aarch32_VLD2_1_T1A1_A
                                when ('1', '01', '10') => __encoding aarch32_VLD3_1_T1A1_A
                                when ('1', '01', '11') => __encoding aarch32_VLD4_1_T1A1_A
                                when ('1', '10', '00') => __encoding aarch32_VLD1_1_T1A1_A
                                when ('1', '10', '01') => __encoding aarch32_VLD2_1_T1A1_A
                                when ('1', '10', '10') => __encoding aarch32_VLD3_1_T1A1_A
                                when ('1', '10', '11') => __encoding aarch32_VLD4_1_T1A1_A
                when (_, '1100', !'1xxx0', _, _, _) =>
                    // ldst
                    case (25 +: 7, 23 +: 2, 21 +: 2, 20 +: 1, 16 +: 4, 12 +: 4, 6 +: 6, 0 +: 6) of
                        when (_, '00', _, _, !'1111', _, '000000', _) => // ldst_unsigned_reg
                            __field size 21 +: 2
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field imm2 4 +: 2
                            __field Rm 0 +: 4
                            case (size, L, Rt) of
                                when ('00', '0', _) => __encoding aarch32_STRB_r_T2_A // STRB_r_T2
                                when ('00', '1', !'1111') => __encoding aarch32_LDRB_r_T2_A // LDRB_r_T2
                                when ('00', '1', '1111') => __encoding aarch32_PLD_r_T1_A // PLD_r_T1
                                when ('01', '0', _) => __encoding aarch32_STRH_r_T2_A // STRH_r_T2
                                when ('01', '1', !'1111') => __encoding aarch32_LDRH_r_T2_A // LDRH_r_T2
                                when ('01', '1', '1111') => __encoding aarch32_PLD_r_T1_A // PLDW_r_T1
                                when ('10', '0', _) => __encoding aarch32_STR_r_T2_A // STR_r_T2
                                when ('10', '1', _) => __encoding aarch32_LDR_r_T2_A // LDR_r_T2
                                when ('11', _, _) => __UNALLOCATED
                        when (_, '00', _, _, !'1111', _, '000001', _) => __UNPREDICTABLE
                        when (_, '00', _, _, !'1111', _, '00001x', _) => __UNPREDICTABLE
                        when (_, '00', _, _, !'1111', _, '0001xx', _) => __UNPREDICTABLE
                        when (_, '00', _, _, !'1111', _, '001xxx', _) => __UNPREDICTABLE
                        when (_, '00', _, _, !'1111', _, '01xxxx', _) => __UNPREDICTABLE
                        when (_, '00', _, _, !'1111', _, '10x0xx', _) => __UNPREDICTABLE
                        when (_, '00', _, _, !'1111', _, '10x1xx', _) => // ldst_unsigned_post
                            __field size 21 +: 2
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field U 9 +: 1
                            __field imm8 0 +: 8
                            case (size, L) of
                                when ('00', '0') => __encoding aarch32_STRB_i_T3_A // STRB_i_T3_post
                                when ('00', '1') => __encoding aarch32_LDRB_i_T3_A // LDRB_i_T3_post
                                when ('01', '0') => __encoding aarch32_STRH_i_T3_A // STRH_i_T3_post
                                when ('01', '1') => __encoding aarch32_LDRH_i_T3_A // LDRH_i_T3_post
                                when ('10', '0') => __encoding aarch32_STR_i_T4_A // STR_i_T4_post
                                when ('10', '1') => __encoding aarch32_LDR_i_T4_A // LDR_i_T4_post
                                when ('11', _) => __UNALLOCATED
                        when (_, '00', _, _, !'1111', _, '1100xx', _) => // ldst_unsigned_nimm
                            __field size 21 +: 2
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field imm8 0 +: 8
                            case (size, L, Rt) of
                                when ('00', '0', _) => __encoding aarch32_STRB_i_T3_A // STRB_i_T3_offn
                                when ('00', '1', !'1111') => __encoding aarch32_LDRB_i_T3_A // LDRB_i_T3_off
                                when ('00', '1', '1111') => __encoding aarch32_PLD_i_T2_A // PLD_i_T2
                                when ('01', '0', _) => __encoding aarch32_STRH_i_T3_A // STRH_i_T3_offn
                                when ('01', '1', !'1111') => __encoding aarch32_LDRH_i_T3_A // LDRH_i_T3_off
                                when ('01', '1', '1111') => __encoding aarch32_PLD_i_T2_A // PLDW_i_T2
                                when ('10', '0', _) => __encoding aarch32_STR_i_T4_A // STR_i_T4_off
                                when ('10', '1', _) => __encoding aarch32_LDR_i_T4_A // LDR_i_T4_off
                                when ('11', _, _) => __UNALLOCATED
                        when (_, '00', _, _, !'1111', _, '1110xx', _) => // ldst_unsigned_unpriv
                            __field size 21 +: 2
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field imm8 0 +: 8
                            case (size, L) of
                                when ('00', '0') => __encoding aarch32_STRBT_T1_A // STRBT_T1
                                when ('00', '1') => __encoding aarch32_LDRBT_T1_A // LDRBT_T1
                                when ('01', '0') => __encoding aarch32_STRHT_T1_A // STRHT_T1
                                when ('01', '1') => __encoding aarch32_LDRHT_T1_A // LDRHT_T1
                                when ('10', '0') => __encoding aarch32_STRT_T1_A // STRT_T1
                                when ('10', '1') => __encoding aarch32_LDRT_T1_A // LDRT_T1
                                when ('11', _) => __UNALLOCATED
                        when (_, '00', _, _, !'1111', _, '11x1xx', _) => // ldst_unsigned_pre
                            __field size 21 +: 2
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field U 9 +: 1
                            __field imm8 0 +: 8
                            case (size, L) of
                                when ('00', '0') => __encoding aarch32_STRB_i_T3_A // STRB_i_T3_pre
                                when ('00', '1') => __encoding aarch32_LDRB_i_T3_A // LDRB_i_T3_pre
                                when ('01', '0') => __encoding aarch32_STRH_i_T3_A // STRH_i_T3_pre
                                when ('01', '1') => __encoding aarch32_LDRH_i_T3_A // LDRH_i_T3_pre
                                when ('10', '0') => __encoding aarch32_STR_i_T4_A // STR_i_T4_pre
                                when ('10', '1') => __encoding aarch32_LDR_i_T4_A // LDR_i_T4_pre
                                when ('11', _) => __UNALLOCATED
                        when (_, '01', _, _, !'1111', _, _, _) => // ldst_unsigned_pimm
                            __field size 21 +: 2
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field imm12 0 +: 12
                            case (size, L, Rt) of
                                when ('00', '0', _) => __encoding aarch32_STRB_i_T2_A // STRB_i_T2
                                when ('00', '1', !'1111') => __encoding aarch32_LDRB_i_T2_A // LDRB_i_T2
                                when ('00', '1', '1111') => __encoding aarch32_PLD_i_T1_A // PLD_i_T1
                                when ('01', '0', _) => __encoding aarch32_STRH_i_T2_A // STRH_i_T2
                                when ('01', '1', !'1111') => __encoding aarch32_LDRH_i_T2_A // LDRH_i_T2
                                when ('01', '1', '1111') => __encoding aarch32_PLD_i_T1_A // PLDW_i_T1
                                when ('10', '0', _) => __encoding aarch32_STR_i_T3_A // STR_i_T3
                                when ('10', '1', _) => __encoding aarch32_LDR_i_T3_A // LDR_i_T3
                        when (_, '0x', _, _, '1111', _, _, _) => // ldlit_unsigned
                            __field U 23 +: 1
                            __field size 21 +: 2
                            __field L 20 +: 1
                            __field Rt 12 +: 4
                            __field imm12 0 +: 12
                            case (size, L, Rt) of
                                when ('0x', '1', '1111') => __encoding aarch32_PLD_l_T1_A // PLD_l_T1
                                when ('00', '1', !'1111') => __encoding aarch32_LDRB_l_T1_A // LDRB_l_T1
                                when ('01', '1', !'1111') => __encoding aarch32_LDRH_l_T1_A // LDRH_l_T1
                                when ('10', '1', _) => __encoding aarch32_LDR_l_T2_A // LDR_l_T2
                                when ('11', _, _) => __UNALLOCATED
                        when (_, '10', _, '1', !'1111', _, '000000', _) => // ldst_signed_reg
                            __field size 21 +: 2
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field imm2 4 +: 2
                            __field Rm 0 +: 4
                            case (size, Rt) of
                                when ('00', !'1111') => __encoding aarch32_LDRSB_r_T2_A // LDRSB_r_T2
                                when ('00', '1111') => __encoding aarch32_PLI_r_T1_A // PLI_r_T1
                                when ('01', !'1111') => __encoding aarch32_LDRSH_r_T2_A // LDRSH_r_T2
                                when ('01', '1111') => __NOP
                                when ('1x', _) => __UNALLOCATED
                        when (_, '10', _, '1', !'1111', _, '000001', _) => __UNPREDICTABLE
                        when (_, '10', _, '1', !'1111', _, '00001x', _) => __UNPREDICTABLE
                        when (_, '10', _, '1', !'1111', _, '0001xx', _) => __UNPREDICTABLE
                        when (_, '10', _, '1', !'1111', _, '001xxx', _) => __UNPREDICTABLE
                        when (_, '10', _, '1', !'1111', _, '01xxxx', _) => __UNPREDICTABLE
                        when (_, '10', _, '1', !'1111', _, '10x0xx', _) => __UNPREDICTABLE
                        when (_, '10', _, '1', !'1111', _, '10x1xx', _) => // ldst_signed_post
                            __field size 21 +: 2
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field U 9 +: 1
                            __field imm8 0 +: 8
                            case (size) of
                                when ('00') => __encoding aarch32_LDRSB_i_T2_A // LDRSB_i_T2_post
                                when ('01') => __encoding aarch32_LDRSH_i_T2_A // LDRSH_i_T2_post
                                when ('1x') => __UNALLOCATED
                        when (_, '10', _, '1', !'1111', _, '1100xx', _) => // ldst_signed_nimm
                            __field size 21 +: 2
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field imm8 0 +: 8
                            case (size, Rt) of
                                when ('00', !'1111') => __encoding aarch32_LDRSB_i_T2_A // LDRSB_i_T2_off
                                when ('00', '1111') => __encoding aarch32_PLI_i_T2_A // PLI_i_T2
                                when ('01', !'1111') => __encoding aarch32_LDRSH_i_T2_A // LDRSH_i_T2_off
                                when ('01', '1111') => __NOP
                                when ('1x', _) => __UNALLOCATED
                        when (_, '10', _, '1', !'1111', _, '1110xx', _) => // ldst_signed_unpriv
                            __field size 21 +: 2
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field imm8 0 +: 8
                            case (size) of
                                when ('00') => __encoding aarch32_LDRSBT_T1_A // LDRSBT_T1
                                when ('01') => __encoding aarch32_LDRSHT_T1_A // LDRSHT_T1
                                when ('1x') => __UNALLOCATED
                        when (_, '10', _, '1', !'1111', _, '11x1xx', _) => // ldst_signed_pre
                            __field size 21 +: 2
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field U 9 +: 1
                            __field imm8 0 +: 8
                            case (size) of
                                when ('00') => __encoding aarch32_LDRSB_i_T2_A // LDRSB_i_T2_pre
                                when ('01') => __encoding aarch32_LDRSH_i_T2_A // LDRSH_i_T2_pre
                                when ('1x') => __UNALLOCATED
                        when (_, '11', _, '1', !'1111', _, _, _) => // ldst_signed_pimm
                            __field size 21 +: 2
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field imm12 0 +: 12
                            case (size, Rt) of
                                when ('00', !'1111') => __encoding aarch32_LDRSB_i_T1_A // LDRSB_i_T1
                                when ('00', '1111') => __encoding aarch32_PLI_i_T1_A // PLI_i_T1
                                when ('01', !'1111') => __encoding aarch32_LDRSH_i_T1_A // LDRSH_i_T1
                                when ('01', '1111') => __NOP
                        when (_, '1x', _, '1', '1111', _, _, _) => // ldlit_signed
                            __field U 23 +: 1
                            __field size 21 +: 2
                            __field Rt 12 +: 4
                            __field imm12 0 +: 12
                            case (size, Rt) of
                                when ('00', !'1111') => __encoding aarch32_LDRSB_l_T1_A // LDRSB_l_T1
                                when ('00', '1111') => __encoding aarch32_PLI_i_T3_A // PLI_i_T3
                                when ('01', !'1111') => __encoding aarch32_LDRSH_l_T1_A // LDRSH_l_T1
                                when ('01', '1111') => __NOP
                                when ('1x', _) => __UNALLOCATED
                when (_, '1101', '0xxxx', _, _, _) =>
                    // reg
                    case (24 +: 8, 23 +: 1, 16 +: 7, 12 +: 4, 8 +: 4, 4 +: 4, 0 +: 4) of
                        when (_, '0', _, _, _, '0000', _) => // shiftr
                            __field type1 21 +: 2
                            __field S 20 +: 1
                            __field Rm 16 +: 4
                            __field Rd 8 +: 4
                            __field Rs 0 +: 4
                            case () of
                                when () => __encoding aarch32_MOV_rr_T2_A // MOVS_rr_T2
                        when (_, '0', _, _, _, '0001', _) => __UNPREDICTABLE
                        when (_, '0', _, _, _, '001x', _) => __UNPREDICTABLE
                        when (_, '0', _, _, _, '01xx', _) => __UNPREDICTABLE
                        when (_, '0', _, _, _, '1xxx', _) => // extendr
                            __field op1 21 +: 2
                            __field U 20 +: 1
                            __field Rn 16 +: 4
                            __field Rd 8 +: 4
                            __field rotate 4 +: 2
                            __field Rm 0 +: 4
                            case (op1, U, Rn) of
                                when ('00', '0', !'1111') => __encoding aarch32_SXTAH_T1_A // SXTAH_T1
                                when ('00', '0', '1111') => __encoding aarch32_SXTH_T2_A // SXTH_T2
                                when ('00', '1', !'1111') => __encoding aarch32_UXTAH_T1_A // UXTAH_T1
                                when ('00', '1', '1111') => __encoding aarch32_UXTH_T2_A // UXTH_T2
                                when ('01', '0', !'1111') => __encoding aarch32_SXTAB16_T1_A // SXTAB16_T1
                                when ('01', '0', '1111') => __encoding aarch32_SXTB16_T1_A // SXTB16_T1
                                when ('01', '1', !'1111') => __encoding aarch32_UXTAB16_T1_A // UXTAB16_T1
                                when ('01', '1', '1111') => __encoding aarch32_UXTB16_T1_A // UXTB16_T1
                                when ('10', '0', !'1111') => __encoding aarch32_SXTAB_T1_A // SXTAB_T1
                                when ('10', '0', '1111') => __encoding aarch32_SXTB_T2_A // SXTB_T2
                                when ('10', '1', !'1111') => __encoding aarch32_UXTAB_T1_A // UXTAB_T1
                                when ('10', '1', '1111') => __encoding aarch32_UXTB_T2_A // UXTB_T2
                                when ('11', _, _) => __UNALLOCATED
                        when (_, '1', _, _, _, '0xxx', _) => // addsub_par
                            __field op1 20 +: 3
                            __field Rn 16 +: 4
                            __field Rd 8 +: 4
                            __field U 6 +: 1
                            __field H 5 +: 1
                            __field S 4 +: 1
                            __field Rm 0 +: 4
                            case (op1, U, H, S) of
                                when ('000', '0', '0', '0') => __encoding aarch32_SADD8_T1_A // SADD8_T1
                                when ('000', '0', '0', '1') => __encoding aarch32_QADD8_T1_A // QADD8_T1
                                when ('000', '0', '1', '0') => __encoding aarch32_SHADD8_T1_A // SHADD8_T1
                                when ('000', '0', '1', '1') => __UNALLOCATED
                                when ('000', '1', '0', '0') => __encoding aarch32_UADD8_T1_A // UADD8_T1
                                when ('000', '1', '0', '1') => __encoding aarch32_UQADD8_T1_A // UQADD8_T1
                                when ('000', '1', '1', '0') => __encoding aarch32_UHADD8_T1_A // UHADD8_T1
                                when ('000', '1', '1', '1') => __UNALLOCATED
                                when ('001', '0', '0', '0') => __encoding aarch32_SADD16_T1_A // SADD16_T1
                                when ('001', '0', '0', '1') => __encoding aarch32_QADD16_T1_A // QADD16_T1
                                when ('001', '0', '1', '0') => __encoding aarch32_SHADD16_T1_A // SHADD16_T1
                                when ('001', '0', '1', '1') => __UNALLOCATED
                                when ('001', '1', '0', '0') => __encoding aarch32_UADD16_T1_A // UADD16_T1
                                when ('001', '1', '0', '1') => __encoding aarch32_UQADD16_T1_A // UQADD16_T1
                                when ('001', '1', '1', '0') => __encoding aarch32_UHADD16_T1_A // UHADD16_T1
                                when ('001', '1', '1', '1') => __UNALLOCATED
                                when ('010', '0', '0', '0') => __encoding aarch32_SASX_T1_A // SASX_T1
                                when ('010', '0', '0', '1') => __encoding aarch32_QASX_T1_A // QASX_T1
                                when ('010', '0', '1', '0') => __encoding aarch32_SHASX_T1_A // SHASX_T1
                                when ('010', '0', '1', '1') => __UNALLOCATED
                                when ('010', '1', '0', '0') => __encoding aarch32_UASX_T1_A // UASX_T1
                                when ('010', '1', '0', '1') => __encoding aarch32_UQASX_T1_A // UQASX_T1
                                when ('010', '1', '1', '0') => __encoding aarch32_UHASX_T1_A // UHASX_T1
                                when ('010', '1', '1', '1') => __UNALLOCATED
                                when ('100', '0', '0', '0') => __encoding aarch32_SSUB8_T1_A // SSUB8_T1
                                when ('100', '0', '0', '1') => __encoding aarch32_QSUB8_T1_A // QSUB8_T1
                                when ('100', '0', '1', '0') => __encoding aarch32_SHSUB8_T1_A // SHSUB8_T1
                                when ('100', '0', '1', '1') => __UNALLOCATED
                                when ('100', '1', '0', '0') => __encoding aarch32_USUB8_T1_A // USUB8_T1
                                when ('100', '1', '0', '1') => __encoding aarch32_UQSUB8_T1_A // UQSUB8_T1
                                when ('100', '1', '1', '0') => __encoding aarch32_UHSUB8_T1_A // UHSUB8_T1
                                when ('100', '1', '1', '1') => __UNALLOCATED
                                when ('101', '0', '0', '0') => __encoding aarch32_SSUB16_T1_A // SSUB16_T1
                                when ('101', '0', '0', '1') => __encoding aarch32_QSUB16_T1_A // QSUB16_T1
                                when ('101', '0', '1', '0') => __encoding aarch32_SHSUB16_T1_A // SHSUB16_T1
                                when ('101', '0', '1', '1') => __UNALLOCATED
                                when ('101', '1', '0', '0') => __encoding aarch32_USUB16_T1_A // USUB16_T1
                                when ('101', '1', '0', '1') => __encoding aarch32_UQSUB16_T1_A // UQSUB16_T1
                                when ('101', '1', '1', '0') => __encoding aarch32_UHSUB16_T1_A // UHSUB16_T1
                                when ('101', '1', '1', '1') => __UNALLOCATED
                                when ('110', '0', '0', '0') => __encoding aarch32_SSAX_T1_A // SSAX_T1
                                when ('110', '0', '0', '1') => __encoding aarch32_QSAX_T1_A // QSAX_T1
                                when ('110', '0', '1', '0') => __encoding aarch32_SHSAX_T1_A // SHSAX_T1
                                when ('110', '0', '1', '1') => __UNALLOCATED
                                when ('110', '1', '0', '0') => __encoding aarch32_USAX_T1_A // USAX_T1
                                when ('110', '1', '0', '1') => __encoding aarch32_UQSAX_T1_A // UQSAX_T1
                                when ('110', '1', '1', '0') => __encoding aarch32_UHSAX_T1_A // UHSAX_T1
                                when ('110', '1', '1', '1') => __UNALLOCATED
                                when ('111', _, _, _) => __UNALLOCATED
                        when (_, '1', _, _, _, '10xx', _) => // dpint_2r
                            __field op1 20 +: 3
                            __field Rn 16 +: 4
                            __field Rd 8 +: 4
                            __field op2 4 +: 2
                            __field Rm 0 +: 4
                            case (op1, op2) of
                                when ('000', '00') => __encoding aarch32_QADD_T1_A // QADD_T1
                                when ('000', '01') => __encoding aarch32_QDADD_T1_A // QDADD_T1
                                when ('000', '10') => __encoding aarch32_QSUB_T1_A // QSUB_T1
                                when ('000', '11') => __encoding aarch32_QDSUB_T1_A // QDSUB_T1
                                when ('001', '00') => __encoding aarch32_REV_T2_A // REV_T2
                                when ('001', '01') => __encoding aarch32_REV16_T2_A // REV16_T2
                                when ('001', '10') => __encoding aarch32_RBIT_T1_A // RBIT_T1
                                when ('001', '11') => __encoding aarch32_REVSH_T2_A // REVSH_T2
                                when ('010', '00') => __encoding aarch32_SEL_T1_A // SEL_T1
                                when ('010', '01') => __UNALLOCATED
                                when ('010', '1x') => __UNALLOCATED
                                when ('011', '00') => __encoding aarch32_CLZ_T1_A // CLZ_T1
                                when ('011', '01') => __UNALLOCATED
                                when ('011', '1x') => __UNALLOCATED
                                when ('100', '00') => __encoding aarch32_CRC32_T1_A // CRC32B_T1
                                when ('100', '01') => __encoding aarch32_CRC32_T1_A // CRC32H_T1
                                when ('100', '10') => __encoding aarch32_CRC32_T1_A // CRC32W_T1
                                when ('100', '11') => __UNPREDICTABLE
                                when ('101', '00') => __encoding aarch32_CRC32_T1_A // CRC32CB_T1
                                when ('101', '01') => __encoding aarch32_CRC32_T1_A // CRC32CH_T1
                                when ('101', '10') => __encoding aarch32_CRC32_T1_A // CRC32CW_T1
                                when ('101', '11') => __UNPREDICTABLE
                                when ('11x', _) => __UNALLOCATED
                        when (_, '1', _, _, _, '11xx', _) => __UNPREDICTABLE
                when (_, '1101', '10xxx', _, _, _) =>
                    // mul
                    case (23 +: 9, 8 +: 15, 6 +: 2, 0 +: 6) of
                        when (_, _, '00', _) => // mul_abd
                            __field op1 20 +: 3
                            __field Rn 16 +: 4
                            __field Ra 12 +: 4
                            __field Rd 8 +: 4
                            __field op2 4 +: 2
                            __field Rm 0 +: 4
                            case (op1, Ra, op2) of
                                when ('000', !'1111', '00') => __encoding aarch32_MLA_T1_A // MLA_T1
                                when ('000', _, '01') => __encoding aarch32_MLS_T1_A // MLS_T1
                                when ('000', _, '1x') => __UNALLOCATED
                                when ('000', '1111', '00') => __encoding aarch32_MUL_T2_A // MUL_T2
                                when ('001', !'1111', '00') => __encoding aarch32_SMLABB_T1_A // SMLABB_T1
                                when ('001', !'1111', '01') => __encoding aarch32_SMLABB_T1_A // SMLABT_T1
                                when ('001', !'1111', '10') => __encoding aarch32_SMLABB_T1_A // SMLATB_T1
                                when ('001', !'1111', '11') => __encoding aarch32_SMLABB_T1_A // SMLATT_T1
                                when ('001', '1111', '00') => __encoding aarch32_SMULBB_T1_A // SMULBB_T1
                                when ('001', '1111', '01') => __encoding aarch32_SMULBB_T1_A // SMULBT_T1
                                when ('001', '1111', '10') => __encoding aarch32_SMULBB_T1_A // SMULTB_T1
                                when ('001', '1111', '11') => __encoding aarch32_SMULBB_T1_A // SMULTT_T1
                                when ('010', !'1111', '00') => __encoding aarch32_SMLAD_T1_A // SMLAD_T1
                                when ('010', !'1111', '01') => __encoding aarch32_SMLAD_T1_A // SMLADX_T1
                                when ('010', _, '1x') => __UNALLOCATED
                                when ('010', '1111', '00') => __encoding aarch32_SMUAD_T1_A // SMUAD_T1
                                when ('010', '1111', '01') => __encoding aarch32_SMUAD_T1_A // SMUADX_T1
                                when ('011', !'1111', '00') => __encoding aarch32_SMLAWB_T1_A // SMLAWB_T1
                                when ('011', !'1111', '01') => __encoding aarch32_SMLAWB_T1_A // SMLAWT_T1
                                when ('011', _, '1x') => __UNALLOCATED
                                when ('011', '1111', '00') => __encoding aarch32_SMULWB_T1_A // SMULWB_T1
                                when ('011', '1111', '01') => __encoding aarch32_SMULWB_T1_A // SMULWT_T1
                                when ('100', !'1111', '00') => __encoding aarch32_SMLSD_T1_A // SMLSD_T1
                                when ('100', !'1111', '01') => __encoding aarch32_SMLSD_T1_A // SMLSDX_T1
                                when ('100', _, '1x') => __UNALLOCATED
                                when ('100', '1111', '00') => __encoding aarch32_SMUSD_T1_A // SMUSD_T1
                                when ('100', '1111', '01') => __encoding aarch32_SMUSD_T1_A // SMUSDX_T1
                                when ('101', !'1111', '00') => __encoding aarch32_SMMLA_T1_A // SMMLA_T1
                                when ('101', !'1111', '01') => __encoding aarch32_SMMLA_T1_A // SMMLAR_T1
                                when ('101', _, '1x') => __UNALLOCATED
                                when ('101', '1111', '00') => __encoding aarch32_SMMUL_T1_A // SMMUL_T1
                                when ('101', '1111', '01') => __encoding aarch32_SMMUL_T1_A // SMMULR_T1
                                when ('110', _, '00') => __encoding aarch32_SMMLS_T1_A // SMMLS_T1
                                when ('110', _, '01') => __encoding aarch32_SMMLS_T1_A // SMMLSR_T1
                                when ('110', _, '1x') => __UNALLOCATED
                                when ('111', !'1111', '00') => __encoding aarch32_USADA8_T1_A // USADA8_T1
                                when ('111', _, '01') => __UNALLOCATED
                                when ('111', _, '1x') => __UNALLOCATED
                                when ('111', '1111', '00') => __encoding aarch32_USAD8_T1_A // USAD8_T1
                        when (_, _, '01', _) => __UNPREDICTABLE
                        when (_, _, '1x', _) => __UNPREDICTABLE
                when (_, '1101', '11xxx', _, _, _) => // lmul_div
                    __field op1 20 +: 3
                    __field Rn 16 +: 4
                    __field RdLo 12 +: 4
                    __field RdHi 8 +: 4
                    __field op2 4 +: 4
                    __field Rm 0 +: 4
                    case (op1, op2) of
                        when ('000', !'0000') => __UNALLOCATED
                        when ('000', '0000') => __encoding aarch32_SMULL_T1_A // SMULL_T1
                        when ('001', !'1111') => __UNALLOCATED
                        when ('001', '1111') => __encoding aarch32_SDIV_T1_A // SDIV_T1
                        when ('010', !'0000') => __UNALLOCATED
                        when ('010', '0000') => __encoding aarch32_UMULL_T1_A // UMULL_T1
                        when ('011', !'1111') => __UNALLOCATED
                        when ('011', '1111') => __encoding aarch32_UDIV_T1_A // UDIV_T1
                        when ('100', '0000') => __encoding aarch32_SMLAL_T1_A // SMLAL_T1
                        when ('100', '0001') => __UNALLOCATED
                        when ('100', '001x') => __UNALLOCATED
                        when ('100', '01xx') => __UNALLOCATED
                        when ('100', '1000') => __encoding aarch32_SMLALBB_T1_A // SMLALBB_T1
                        when ('100', '1001') => __encoding aarch32_SMLALBB_T1_A // SMLALBT_T1
                        when ('100', '1010') => __encoding aarch32_SMLALBB_T1_A // SMLALTB_T1
                        when ('100', '1011') => __encoding aarch32_SMLALBB_T1_A // SMLALTT_T1
                        when ('100', '1100') => __encoding aarch32_SMLALD_T1_A // SMLALD_T1
                        when ('100', '1101') => __encoding aarch32_SMLALD_T1_A // SMLALDX_T1
                        when ('100', '111x') => __UNALLOCATED
                        when ('101', '0xxx') => __UNALLOCATED
                        when ('101', '10xx') => __UNALLOCATED
                        when ('101', '1100') => __encoding aarch32_SMLSLD_T1_A // SMLSLD_T1
                        when ('101', '1101') => __encoding aarch32_SMLSLD_T1_A // SMLSLDX_T1
                        when ('101', '111x') => __UNALLOCATED
                        when ('110', '0000') => __encoding aarch32_UMLAL_T1_A // UMLAL_T1
                        when ('110', '0001') => __UNALLOCATED
                        when ('110', '001x') => __UNALLOCATED
                        when ('110', '010x') => __UNALLOCATED
                        when ('110', '0110') => __encoding aarch32_UMAAL_T1_A // UMAAL_T1
                        when ('110', '0111') => __UNALLOCATED
                        when ('110', '1xxx') => __UNALLOCATED
                        when ('111', _) => __UNALLOCATED
__decode A32
    // A32
    case (28 +: 4, 25 +: 3, 5 +: 20, 4 +: 1, 0 +: 4) of
        when (!'1111', '00x', _, _, _) =>
            // dp
            case (28 +: 4, 26 +: 2, 25 +: 1, 20 +: 5, 8 +: 12, 7 +: 1, 5 +: 2, 4 +: 1, 0 +: 4) of
                when (_, _, '0', _, _, '1', !'00', '1', _) =>
                    // xldst
                    case (28 +: 4, 25 +: 3, 23 +: 2, 22 +: 1, 8 +: 14, 7 +: 1, 5 +: 2, 4 +: 1, 0 +: 4) of
                        when (_, _, _, '0', _, _, _, _, _) => // ldstxreg
                            __field cond 28 +: 4
                            __field P 24 +: 1
                            __field U 23 +: 1
                            __field W 21 +: 1
                            __field o1 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field op2 5 +: 2
                            __field Rm 0 +: 4
                            case (P, W, o1, op2) of
                                when ('0', '0', '0', '01') => __encoding aarch32_STRH_r_A1_A // STRH_r_A1_post
                                when ('0', '0', '0', '10') => __encoding aarch32_LDRD_r_A1_A // LDRD_r_A1_post
                                when ('0', '0', '0', '11') => __encoding aarch32_STRD_r_A1_A // STRD_r_A1_post
                                when ('0', '0', '1', '01') => __encoding aarch32_LDRH_r_A1_A // LDRH_r_A1_post
                                when ('0', '0', '1', '10') => __encoding aarch32_LDRSB_r_A1_A // LDRSB_r_A1_post
                                when ('0', '0', '1', '11') => __encoding aarch32_LDRSH_r_A1_A // LDRSH_r_A1_post
                                when ('0', '1', '0', '01') => __encoding aarch32_STRHT_A2_A // STRHT_A2
                                when ('0', '1', '0', '10') => __UNALLOCATED
                                when ('0', '1', '0', '11') => __UNALLOCATED
                                when ('0', '1', '1', '01') => __encoding aarch32_LDRHT_A2_A // LDRHT_A2
                                when ('0', '1', '1', '10') => __encoding aarch32_LDRSBT_A2_A // LDRSBT_A2
                                when ('0', '1', '1', '11') => __encoding aarch32_LDRSHT_A2_A // LDRSHT_A2
                                when ('1', _, '0', '01') => __encoding aarch32_STRH_r_A1_A // STRH_r_A1_pre
                                when ('1', _, '0', '10') => __encoding aarch32_LDRD_r_A1_A // LDRD_r_A1_pre
                                when ('1', _, '0', '11') => __encoding aarch32_STRD_r_A1_A // STRD_r_A1_pre
                                when ('1', _, '1', '01') => __encoding aarch32_LDRH_r_A1_A // LDRH_r_A1_pre
                                when ('1', _, '1', '10') => __encoding aarch32_LDRSB_r_A1_A // LDRSB_r_A1_pre
                                when ('1', _, '1', '11') => __encoding aarch32_LDRSH_r_A1_A // LDRSH_r_A1_pre
                        when (_, _, _, '1', _, _, _, _, _) => // ldstximm
                            __field cond 28 +: 4
                            __field P 24 +: 1
                            __field U 23 +: 1
                            __field W 21 +: 1
                            __field o1 20 +: 1
                            __field Rn 16 +: 4
                            __field Rt 12 +: 4
                            __field imm4H 8 +: 4
                            __field op2 5 +: 2
                            __field imm4L 0 +: 4
                            case (P:W, o1, Rn, op2) of
                                when (_, '0', '1111', '10') => __encoding aarch32_LDRD_l_A1_A // LDRD_l_A1
                                when (!'01', '1', '1111', '01') => __encoding aarch32_LDRH_l_A1_A // LDRH_l_A1
                                when (!'01', '1', '1111', '10') => __encoding aarch32_LDRSB_l_A1_A // LDRSB_l_A1
                                when (!'01', '1', '1111', '11') => __encoding aarch32_LDRSH_l_A1_A // LDRSH_l_A1
                                when ('00', '0', !'1111', '10') => __encoding aarch32_LDRD_i_A1_A // LDRD_i_A1_post
                                when ('00', '0', _, '01') => __encoding aarch32_STRH_i_A1_A // STRH_i_A1_post
                                when ('00', '0', _, '11') => __encoding aarch32_STRD_i_A1_A // STRD_i_A1_post
                                when ('00', '1', !'1111', '01') => __encoding aarch32_LDRH_i_A1_A // LDRH_i_A1_post
                                when ('00', '1', !'1111', '10') => __encoding aarch32_LDRSB_i_A1_A // LDRSB_i_A1_post
                                when ('00', '1', !'1111', '11') => __encoding aarch32_LDRSH_i_A1_A // LDRSH_i_A1_post
                                when ('01', '0', !'1111', '10') => __UNALLOCATED
                                when ('01', '0', _, '01') => __encoding aarch32_STRHT_A1_A // STRHT_A1
                                when ('01', '0', _, '11') => __UNALLOCATED
                                when ('01', '1', _, '01') => __encoding aarch32_LDRHT_A1_A // LDRHT_A1
                                when ('01', '1', _, '10') => __encoding aarch32_LDRSBT_A1_A // LDRSBT_A1
                                when ('01', '1', _, '11') => __encoding aarch32_LDRSHT_A1_A // LDRSHT_A1
                                when ('10', '0', !'1111', '10') => __encoding aarch32_LDRD_i_A1_A // LDRD_i_A1_off
                                when ('10', '0', _, '01') => __encoding aarch32_STRH_i_A1_A // STRH_i_A1_off
                                when ('10', '0', _, '11') => __encoding aarch32_STRD_i_A1_A // STRD_i_A1_off
                                when ('10', '1', !'1111', '01') => __encoding aarch32_LDRH_i_A1_A // LDRH_i_A1_off
                                when ('10', '1', !'1111', '10') => __encoding aarch32_LDRSB_i_A1_A // LDRSB_i_A1_off
                                when ('10', '1', !'1111', '11') => __encoding aarch32_LDRSH_i_A1_A // LDRSH_i_A1_off
                                when ('11', '0', !'1111', '10') => __encoding aarch32_LDRD_i_A1_A // LDRD_i_A1_pre
                                when ('11', '0', _, '01') => __encoding aarch32_STRH_i_A1_A // STRH_i_A1_pre
                                when ('11', '0', _, '11') => __encoding aarch32_STRD_i_A1_A // STRD_i_A1_pre
                                when ('11', '1', !'1111', '01') => __encoding aarch32_LDRH_i_A1_A // LDRH_i_A1_pre
                                when ('11', '1', !'1111', '10') => __encoding aarch32_LDRSB_i_A1_A // LDRSB_i_A1_pre
                                when ('11', '1', !'1111', '11') => __encoding aarch32_LDRSH_i_A1_A // LDRSH_i_A1_pre
                when (_, _, '0', '0xxxx', _, '1', '00', '1', _) => // mul_word
                    __field cond 28 +: 4
                    __field opc 21 +: 3
                    __field S 20 +: 1
                    __field RdHi 16 +: 4
                    __field RdLo 12 +: 4
                    __field Rm 8 +: 4
                    __field Rn 0 +: 4
                    case (opc, S) of
                        when ('000', _) => __encoding aarch32_MUL_A1_A // MULS_A1
                        when ('001', _) => __encoding aarch32_MLA_A1_A // MLAS_A1
                        when ('010', '0') => __encoding aarch32_UMAAL_A1_A // UMAAL_A1
                        when ('010', '1') => __UNALLOCATED
                        when ('011', '0') => __encoding aarch32_MLS_A1_A // MLS_A1
                        when ('011', '1') => __UNALLOCATED
                        when ('100', _) => __encoding aarch32_UMULL_A1_A // UMULLS_A1
                        when ('101', _) => __encoding aarch32_UMLAL_A1_A // UMLALS_A1
                        when ('110', _) => __encoding aarch32_SMULL_A1_A // SMULLS_A1
                        when ('111', _) => __encoding aarch32_SMLAL_A1_A // SMLALS_A1
                when (_, _, '0', '1xxxx', _, '1', '00', '1', _) =>
                    // sync
                    case (28 +: 4, 24 +: 4, 23 +: 1, 12 +: 11, 10 +: 2, 8 +: 2, 4 +: 4, 0 +: 4) of
                        when (_, _, '0', _, _, _, _, _) => __UNPREDICTABLE
                        when (_, _, '1', _, _, _, _, _) => // ldst_excl
                            __field cond 28 +: 4
                            __field type1 21 +: 2
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field xRd 12 +: 4
                            __field ex 9 +: 1
                            __field ord 8 +: 1
                            __field xRt 0 +: 4
                            case (type1, L, ex, ord) of
                                when ('00', '0', '0', '0') => __encoding aarch32_STL_A1_A // STL_A1
                                when ('00', '0', '0', '1') => __UNALLOCATED
                                when ('00', '0', '1', '0') => __encoding aarch32_STLEX_A1_A // STLEX_A1
                                when ('00', '0', '1', '1') => __encoding aarch32_STREX_A1_A // STREX_A1
                                when ('00', '1', '0', '0') => __encoding aarch32_LDA_A1_A // LDA_A1
                                when ('00', '1', '0', '1') => __UNALLOCATED
                                when ('00', '1', '1', '0') => __encoding aarch32_LDAEX_A1_A // LDAEX_A1
                                when ('00', '1', '1', '1') => __encoding aarch32_LDREX_A1_A // LDREX_A1
                                when ('01', '0', '0', _) => __UNALLOCATED
                                when ('01', '0', '1', '0') => __encoding aarch32_STLEXD_A1_A // STLEXD_A1
                                when ('01', '0', '1', '1') => __encoding aarch32_STREXD_A1_A // STREXD_A1
                                when ('01', '1', '0', _) => __UNALLOCATED
                                when ('01', '1', '1', '0') => __encoding aarch32_LDAEXD_A1_A // LDAEXD_A1
                                when ('01', '1', '1', '1') => __encoding aarch32_LDREXD_A1_A // LDREXD_A1
                                when ('10', '0', '0', '0') => __encoding aarch32_STLB_A1_A // STLB_A1
                                when ('10', '0', '0', '1') => __UNALLOCATED
                                when ('10', '0', '1', '0') => __encoding aarch32_STLEXB_A1_A // STLEXB_A1
                                when ('10', '0', '1', '1') => __encoding aarch32_STREXB_A1_A // STREXB_A1
                                when ('10', '1', '0', '0') => __encoding aarch32_LDAB_A1_A // LDAB_A1
                                when ('10', '1', '0', '1') => __UNALLOCATED
                                when ('10', '1', '1', '0') => __encoding aarch32_LDAEXB_A1_A // LDAEXB_A1
                                when ('10', '1', '1', '1') => __encoding aarch32_LDREXB_A1_A // LDREXB_A1
                                when ('11', '0', '0', '0') => __encoding aarch32_STLH_A1_A // STLH_A1
                                when ('11', '0', '0', '1') => __UNALLOCATED
                                when ('11', '0', '1', '0') => __encoding aarch32_STLEXH_A1_A // STLEXH_A1
                                when ('11', '0', '1', '1') => __encoding aarch32_STREXH_A1_A // STREXH_A1
                                when ('11', '1', '0', '0') => __encoding aarch32_LDAH_A1_A // LDAH_A1
                                when ('11', '1', '0', '1') => __UNALLOCATED
                                when ('11', '1', '1', '0') => __encoding aarch32_LDAEXH_A1_A // LDAEXH_A1
                                when ('11', '1', '1', '1') => __encoding aarch32_LDREXH_A1_A // LDREXH_A1
                when (_, _, '0', '10xx0', _, '0', _, _, _) =>
                    // dpmisc
                    case (28 +: 4, 23 +: 5, 21 +: 2, 20 +: 1, 8 +: 12, 7 +: 1, 4 +: 3, 0 +: 4) of
                        when (_, _, '00', _, _, _, '001', _) => __UNPREDICTABLE
                        when (_, _, '00', _, _, _, '010', _) => __UNPREDICTABLE
                        when (_, _, '00', _, _, _, '011', _) => __UNPREDICTABLE
                        when (_, _, '00', _, _, _, '110', _) => __UNPREDICTABLE
                        when (_, _, '01', _, _, _, '001', _) => // bx_reg
                            __field cond 28 +: 4
                            __field Rm 0 +: 4
                            case () of
                                when () => __encoding aarch32_BX_A1_A // BX_A1
                        when (_, _, '01', _, _, _, '010', _) => // bxj_reg
                            __field cond 28 +: 4
                            __field Rm 0 +: 4
                            case () of
                                when () => __encoding aarch32_BXJ_A1_A // BXJ_A1
                        when (_, _, '01', _, _, _, '011', _) => // blx_reg
                            __field cond 28 +: 4
                            __field Rm 0 +: 4
                            case () of
                                when () => __encoding aarch32_BLX_r_A1_A // BLX_r_A1
                        when (_, _, '01', _, _, _, '110', _) => __UNPREDICTABLE
                        when (_, _, '10', _, _, _, '001', _) => __UNPREDICTABLE
                        when (_, _, '10', _, _, _, '010', _) => __UNPREDICTABLE
                        when (_, _, '10', _, _, _, '011', _) => __UNPREDICTABLE
                        when (_, _, '10', _, _, _, '110', _) => __UNPREDICTABLE
                        when (_, _, '11', _, _, _, '001', _) => // clz
                            __field cond 28 +: 4
                            __field Rd 12 +: 4
                            __field Rm 0 +: 4
                            case () of
                                when () => __encoding aarch32_CLZ_A1_A // CLZ_A1
                        when (_, _, '11', _, _, _, '010', _) => __UNPREDICTABLE
                        when (_, _, '11', _, _, _, '011', _) => __UNPREDICTABLE
                        when (_, _, '11', _, _, _, '110', _) => // eret
                            __field cond 28 +: 4
                            case () of
                                when () => __encoding aarch32_ERET_A1_A // ERET_A1
                        when (_, _, _, _, _, _, '111', _) => // except
                            __field cond 28 +: 4
                            __field opc 21 +: 2
                            __field imm12 8 +: 12
                            __field imm4 0 +: 4
                            case (opc) of
                                when ('00') => __encoding aarch32_HLT_A1_A // HLT_A1
                                when ('01') => __encoding aarch32_BKPT_A1_A // BKPT_A1
                                when ('10') => __encoding aarch32_HVC_A1_A // HVC_A1
                                when ('11') => __encoding aarch32_SMC_A1_AS // SMC_A1_AS
                        when (_, _, _, _, _, _, '000', _) => // movsr_reg
                            __field cond 28 +: 4
                            __field opc 21 +: 2
                            __field mask 16 +: 4
                            __field Rd 12 +: 4
                            __field B 9 +: 1
                            __field m 8 +: 1
                            __field Rn 0 +: 4
                            case (opc, B) of
                                when ('x0', '0') => __encoding aarch32_MRS_A1_AS // MRS_A1_AS
                                when ('x0', '1') => __encoding aarch32_MRS_br_A1_AS // MRS_br_A1_AS
                                when ('x1', '0') => __encoding aarch32_MSR_r_A1_AS // MSR_r_A1_AS
                                when ('x1', '1') => __encoding aarch32_MSR_br_A1_AS // MSR_br_A1_AS
                        when (_, _, _, _, _, _, '100', _) => // crc32
                            __field cond 28 +: 4
                            __field sz 21 +: 2
                            __field Rn 16 +: 4
                            __field Rd 12 +: 4
                            __field C 9 +: 1
                            __field Rm 0 +: 4
                            case (sz, C) of
                                when ('00', '0') => __encoding aarch32_CRC32_A1_A // CRC32B_A1
                                when ('00', '1') => __encoding aarch32_CRC32_A1_A // CRC32CB_A1
                                when ('01', '0') => __encoding aarch32_CRC32_A1_A // CRC32H_A1
                                when ('01', '1') => __encoding aarch32_CRC32_A1_A // CRC32CH_A1
                                when ('10', '0') => __encoding aarch32_CRC32_A1_A // CRC32W_A1
                                when ('10', '1') => __encoding aarch32_CRC32_A1_A // CRC32CW_A1
                                when ('11', _) => __UNPREDICTABLE
                        when (_, _, _, _, _, _, '101', _) => // intsat
                            __field cond 28 +: 4
                            __field opc 21 +: 2
                            __field Rn 16 +: 4
                            __field Rd 12 +: 4
                            __field Rm 0 +: 4
                            case (opc) of
                                when ('00') => __encoding aarch32_QADD_A1_A // QADD_A1
                                when ('01') => __encoding aarch32_QSUB_A1_A // QSUB_A1
                                when ('10') => __encoding aarch32_QDADD_A1_A // QDADD_A1
                                when ('11') => __encoding aarch32_QDSUB_A1_A // QDSUB_A1
                when (_, _, '0', '10xx0', _, '1', _, '0', _) => // mul_half
                    __field cond 28 +: 4
                    __field opc 21 +: 2
                    __field Rd 16 +: 4
                    __field Ra 12 +: 4
                    __field Rm 8 +: 4
                    __field M 6 +: 1
                    __field N 5 +: 1
                    __field Rn 0 +: 4
                    case (opc, M, N) of
                        when ('00', _, _) => __encoding aarch32_SMLABB_A1_A // SMLATT_A1
                        when ('01', '0', '0') => __encoding aarch32_SMLAWB_A1_A // SMLAWB_A1
                        when ('01', '0', '1') => __encoding aarch32_SMULWB_A1_A // SMULWB_A1
                        when ('01', '1', '0') => __encoding aarch32_SMLAWB_A1_A // SMLAWT_A1
                        when ('01', '1', '1') => __encoding aarch32_SMULWB_A1_A // SMULWT_A1
                        when ('10', _, _) => __encoding aarch32_SMLALBB_A1_A // SMLALTT_A1
                        when ('11', _, _) => __encoding aarch32_SMULBB_A1_A // SMULTT_A1
                when (_, _, '0', !'10xx0', _, _, _, '0', _) =>
                    // dpregis
                    case (28 +: 4, 25 +: 3, 23 +: 2, 21 +: 2, 20 +: 1, 5 +: 15, 4 +: 1, 0 +: 4) of
                        when (_, _, '0x', _, _, _, _, _) => // intdp3reg_immsh
                            __field cond 28 +: 4
                            __field opc 21 +: 3
                            __field S 20 +: 1
                            __field Rn 16 +: 4
                            __field Rd 12 +: 4
                            __field imm5 7 +: 5
                            __field type1 5 +: 2
                            __field Rm 0 +: 4
                            case (opc, S, Rn) of
                                when ('000', _, _) => __encoding aarch32_AND_r_A1_A // ANDS_r_A1_RRX
                                when ('001', _, _) => __encoding aarch32_EOR_r_A1_A // EORS_r_A1_RRX
                                when ('010', '0', !'1101') => __encoding aarch32_SUB_r_A1_A // SUB_r_A1_RRX
                                when ('010', '0', '1101') => __encoding aarch32_SUB_SP_r_A1_A // SUB_SP_r_A1_RRX
                                when ('010', '1', !'1101') => __encoding aarch32_SUB_r_A1_A // SUBS_r_A1_RRX
                                when ('010', '1', '1101') => __encoding aarch32_SUB_SP_r_A1_A // SUBS_SP_r_A1_RRX
                                when ('011', _, _) => __encoding aarch32_RSB_r_A1_A // RSBS_r_A1_RRX
                                when ('100', '0', !'1101') => __encoding aarch32_ADD_r_A1_A // ADD_r_A1_RRX
                                when ('100', '0', '1101') => __encoding aarch32_ADD_SP_r_A1_A // ADD_SP_r_A1_RRX
                                when ('100', '1', !'1101') => __encoding aarch32_ADD_r_A1_A // ADDS_r_A1_RRX
                                when ('100', '1', '1101') => __encoding aarch32_ADD_SP_r_A1_A // ADDS_SP_r_A1_RRX
                                when ('101', _, _) => __encoding aarch32_ADC_r_A1_A // ADCS_r_A1_RRX
                                when ('110', _, _) => __encoding aarch32_SBC_r_A1_A // SBCS_r_A1_RRX
                                when ('111', _, _) => __encoding aarch32_RSC_r_A1_A // RSCS_r_A1_RRX
                        when (_, _, '10', _, '1', _, _, _) => // intdp2reg_immsh
                            __field cond 28 +: 4
                            __field opc 21 +: 2
                            __field Rn 16 +: 4
                            __field imm5 7 +: 5
                            __field type1 5 +: 2
                            __field Rm 0 +: 4
                            case (opc) of
                                when ('00') => __encoding aarch32_TST_r_A1_A // TST_r_A1_RRX
                                when ('01') => __encoding aarch32_TEQ_r_A1_A // TEQ_r_A1_RRX
                                when ('10') => __encoding aarch32_CMP_r_A1_A // CMP_r_A1_RRX
                                when ('11') => __encoding aarch32_CMN_r_A1_A // CMN_r_A1_RRX
                        when (_, _, '11', _, _, _, _, _) => // logic3reg_immsh
                            __field cond 28 +: 4
                            __field opc 21 +: 2
                            __field S 20 +: 1
                            __field Rn 16 +: 4
                            __field Rd 12 +: 4
                            __field imm5 7 +: 5
                            __field type1 5 +: 2
                            __field Rm 0 +: 4
                            case (opc) of
                                when ('00') => __encoding aarch32_ORR_r_A1_A // ORRS_r_A1_RRX
                                when ('01') => __encoding aarch32_MOV_r_A1_A // MOVS_r_A1_RRX
                                when ('10') => __encoding aarch32_BIC_r_A1_A // BICS_r_A1_RRX
                                when ('11') => __encoding aarch32_MVN_r_A1_A // MVNS_r_A1_RRX
                when (_, _, '0', !'10xx0', _, '0', _, '1', _) =>
                    // dpregrs
                    case (28 +: 4, 25 +: 3, 23 +: 2, 21 +: 2, 20 +: 1, 8 +: 12, 7 +: 1, 5 +: 2, 4 +: 1, 0 +: 4) of
                        when (_, _, '0x', _, _, _, _, _, _, _) => // intdp3reg_regsh
                            __field cond 28 +: 4
                            __field opc 21 +: 3
                            __field S 20 +: 1
                            __field Rn 16 +: 4
                            __field Rd 12 +: 4
                            __field Rs 8 +: 4
                            __field type1 5 +: 2
                            __field Rm 0 +: 4
                            case (opc) of
                                when ('000') => __encoding aarch32_AND_rr_A1_A // ANDS_rr_A1
                                when ('001') => __encoding aarch32_EOR_rr_A1_A // EORS_rr_A1
                                when ('010') => __encoding aarch32_SUB_rr_A1_A // SUBS_rr_A1
                                when ('011') => __encoding aarch32_RSB_rr_A1_A // RSBS_rr_A1
                                when ('100') => __encoding aarch32_ADD_rr_A1_A // ADDS_rr_A1
                                when ('101') => __encoding aarch32_ADC_rr_A1_A // ADCS_rr_A1
                                when ('110') => __encoding aarch32_SBC_rr_A1_A // SBCS_rr_A1
                                when ('111') => __encoding aarch32_RSC_rr_A1_A // RSCS_rr_A1
                        when (_, _, '10', _, '1', _, _, _, _, _) => // intdp2reg_regsh
                            __field cond 28 +: 4
                            __field opc 21 +: 2
                            __field Rn 16 +: 4
                            __field Rs 8 +: 4
                            __field type1 5 +: 2
                            __field Rm 0 +: 4
                            case (opc) of
                                when ('00') => __encoding aarch32_TST_rr_A1_A // TST_rr_A1
                                when ('01') => __encoding aarch32_TEQ_rr_A1_A // TEQ_rr_A1
                                when ('10') => __encoding aarch32_CMP_rr_A1_A // CMP_rr_A1
                                when ('11') => __encoding aarch32_CMN_rr_A1_A // CMN_rr_A1
                        when (_, _, '11', _, _, _, _, _, _, _) => // logic3reg_regsh
                            __field cond 28 +: 4
                            __field opc 21 +: 2
                            __field S 20 +: 1
                            __field Rn 16 +: 4
                            __field Rd 12 +: 4
                            __field Rs 8 +: 4
                            __field type1 5 +: 2
                            __field Rm 0 +: 4
                            case (opc) of
                                when ('00') => __encoding aarch32_ORR_rr_A1_A // ORRS_rr_A1
                                when ('01') => __encoding aarch32_MOV_rr_A1_A // MOVS_rr_A1
                                when ('10') => __encoding aarch32_BIC_rr_A1_A // BICS_rr_A1
                                when ('11') => __encoding aarch32_MVN_rr_A1_A // MVNS_rr_A1
                when (_, _, '1', _, _, _, _, _, _) =>
                    // dpimm
                    case (28 +: 4, 25 +: 3, 23 +: 2, 22 +: 1, 20 +: 2, 0 +: 20) of
                        when (_, _, '0x', _, _, _) => // intdp2reg_imm
                            __field cond 28 +: 4
                            __field opc 21 +: 3
                            __field S 20 +: 1
                            __field Rn 16 +: 4
                            __field Rd 12 +: 4
                            __field imm12 0 +: 12
                            case (opc, S, Rn) of
                                when ('000', _, _) => __encoding aarch32_AND_i_A1_A // ANDS_i_A1
                                when ('001', _, _) => __encoding aarch32_EOR_i_A1_A // EORS_i_A1
                                when ('010', '0', !'11x1') => __encoding aarch32_SUB_i_A1_A // SUB_i_A1
                                when ('010', '0', '1101') => __encoding aarch32_SUB_SP_i_A1_A // SUB_SP_i_A1
                                when ('010', '0', '1111') => __encoding aarch32_ADR_A1_A
                                when ('010', '1', !'1101') => __encoding aarch32_SUB_i_A1_A // SUBS_i_A1
                                when ('010', '1', '1101') => __encoding aarch32_SUB_SP_i_A1_A // SUBS_SP_i_A1
                                when ('011', _, _) => __encoding aarch32_RSB_i_A1_A // RSBS_i_A1
                                when ('100', '0', !'11x1') => __encoding aarch32_ADD_i_A1_A // ADD_i_A1
                                when ('100', '0', '1101') => __encoding aarch32_ADD_SP_i_A1_A // ADD_SP_i_A1
                                when ('100', '0', '1111') => __encoding aarch32_ADR_A1_A
                                when ('100', '1', !'1101') => __encoding aarch32_ADD_i_A1_A // ADDS_i_A1
                                when ('100', '1', '1101') => __encoding aarch32_ADD_SP_i_A1_A // ADDS_SP_i_A1
                                when ('101', _, _) => __encoding aarch32_ADC_i_A1_A // ADCS_i_A1
                                when ('110', _, _) => __encoding aarch32_SBC_i_A1_A // SBCS_i_A1
                                when ('111', _, _) => __encoding aarch32_RSC_i_A1_A // RSCS_i_A1
                        when (_, _, '10', _, '00', _) => // movw
                            __field cond 28 +: 4
                            __field H 22 +: 1
                            __field imm4 16 +: 4
                            __field Rd 12 +: 4
                            __field imm12 0 +: 12
                            case (H) of
                                when ('0') => __encoding aarch32_MOV_i_A2_A // MOV_i_A2
                                when ('1') => __encoding aarch32_MOVT_A1_A // MOVT_A1
                        when (_, _, '10', _, '10', _) => // movsr_hint_imm
                            __field cond 28 +: 4
                            __field R 22 +: 1
                            __field imm4 16 +: 4
                            __field imm12 0 +: 12
                            case (R:imm4, imm12) of
                                when (!'00000', _) => __encoding aarch32_MSR_i_A1_AS // MSR_i_A1_AS
                                when ('00000', 'xxxx00000000') => __encoding aarch32_NOP_A1_A // NOP_A1
                                when ('00000', 'xxxx00000001') => __encoding aarch32_YIELD_A1_A // YIELD_A1
                                when ('00000', 'xxxx00000010') => __encoding aarch32_WFE_A1_A // WFE_A1
                                when ('00000', 'xxxx00000011') => __encoding aarch32_WFI_A1_A // WFI_A1
                                when ('00000', 'xxxx00000100') => __encoding aarch32_SEV_A1_A // SEV_A1
                                when ('00000', 'xxxx00000101') => __encoding aarch32_SEVL_A1_A // SEVL_A1
                                when ('00000', 'xxxx0000011x') => __NOP
                                when ('00000', 'xxxx00001xxx') => __NOP
                                when ('00000', 'xxxx00010000') => __encoding aarch32_ESB_A1_A // ESB_A1
                                when ('00000', 'xxxx00010001') => __NOP
                                when ('00000', 'xxxx00010010') => __encoding aarch32_TSB_A1_A // TSB_A1
                                when ('00000', 'xxxx00010011') => __NOP
                                when ('00000', 'xxxx00010100') => __encoding aarch32_CSDB_A1_A // CSDB_A1
                                when ('00000', 'xxxx00010101') => __NOP
                                when ('00000', 'xxxx00011xxx') => __NOP
                                when ('00000', 'xxxx0001111x') => __NOP
                                when ('00000', 'xxxx001xxxxx') => __NOP
                                when ('00000', 'xxxx01xxxxxx') => __NOP
                                when ('00000', 'xxxx10xxxxxx') => __NOP
                                when ('00000', 'xxxx110xxxxx') => __NOP
                                when ('00000', 'xxxx1110xxxx') => __NOP
                                when ('00000', 'xxxx1111xxxx') => __encoding aarch32_DBG_A1_A // DBG_A1
                        when (_, _, '10', _, 'x1', _) => // intdp1reg_imm
                            __field cond 28 +: 4
                            __field opc 21 +: 2
                            __field Rn 16 +: 4
                            __field imm12 0 +: 12
                            case (opc) of
                                when ('00') => __encoding aarch32_TST_i_A1_A // TST_i_A1
                                when ('01') => __encoding aarch32_TEQ_i_A1_A // TEQ_i_A1
                                when ('10') => __encoding aarch32_CMP_i_A1_A // CMP_i_A1
                                when ('11') => __encoding aarch32_CMN_i_A1_A // CMN_i_A1
                        when (_, _, '11', _, _, _) => // log2reg_imm
                            __field cond 28 +: 4
                            __field opc 21 +: 2
                            __field S 20 +: 1
                            __field Rn 16 +: 4
                            __field Rd 12 +: 4
                            __field imm12 0 +: 12
                            case (opc) of
                                when ('00') => __encoding aarch32_ORR_i_A1_A // ORRS_i_A1
                                when ('01') => __encoding aarch32_MOV_i_A1_A // MOVS_i_A1
                                when ('10') => __encoding aarch32_BIC_i_A1_A // BICS_i_A1
                                when ('11') => __encoding aarch32_MVN_i_A1_A // MVNS_i_A1
        when (!'1111', '010', _, _, _) => // ldstimm
            __field cond 28 +: 4
            __field P 24 +: 1
            __field U 23 +: 1
            __field o2 22 +: 1
            __field W 21 +: 1
            __field o1 20 +: 1
            __field Rn 16 +: 4
            __field Rt 12 +: 4
            __field imm12 0 +: 12
            case (P:W, o2, o1, Rn) of
                when (!'01', '0', '1', '1111') => __encoding aarch32_LDR_l_A1_A // LDR_l_A1
                when (!'01', '1', '1', '1111') => __encoding aarch32_LDRB_l_A1_A // LDRB_l_A1
                when ('00', '0', '0', _) => __encoding aarch32_STR_i_A1_A // STR_i_A1_post
                when ('00', '0', '1', !'1111') => __encoding aarch32_LDR_i_A1_A // LDR_i_A1_post
                when ('00', '1', '0', _) => __encoding aarch32_STRB_i_A1_A // STRB_i_A1_post
                when ('00', '1', '1', !'1111') => __encoding aarch32_LDRB_i_A1_A // LDRB_i_A1_post
                when ('01', '0', '0', _) => __encoding aarch32_STRT_A1_A // STRT_A1
                when ('01', '0', '1', _) => __encoding aarch32_LDRT_A1_A // LDRT_A1
                when ('01', '1', '0', _) => __encoding aarch32_STRBT_A1_A // STRBT_A1
                when ('01', '1', '1', _) => __encoding aarch32_LDRBT_A1_A // LDRBT_A1
                when ('10', '0', '0', _) => __encoding aarch32_STR_i_A1_A // STR_i_A1_off
                when ('10', '0', '1', !'1111') => __encoding aarch32_LDR_i_A1_A // LDR_i_A1_off
                when ('10', '1', '0', _) => __encoding aarch32_STRB_i_A1_A // STRB_i_A1_off
                when ('10', '1', '1', !'1111') => __encoding aarch32_LDRB_i_A1_A // LDRB_i_A1_off
                when ('11', '0', '0', _) => __encoding aarch32_STR_i_A1_A // STR_i_A1_pre
                when ('11', '0', '1', !'1111') => __encoding aarch32_LDR_i_A1_A // LDR_i_A1_pre
                when ('11', '1', '0', _) => __encoding aarch32_STRB_i_A1_A // STRB_i_A1_pre
                when ('11', '1', '1', !'1111') => __encoding aarch32_LDRB_i_A1_A // LDRB_i_A1_pre
        when (!'1111', '011', _, '0', _) => // ldstreg
            __field cond 28 +: 4
            __field P 24 +: 1
            __field U 23 +: 1
            __field o2 22 +: 1
            __field W 21 +: 1
            __field o1 20 +: 1
            __field Rn 16 +: 4
            __field Rt 12 +: 4
            __field imm5 7 +: 5
            __field type1 5 +: 2
            __field Rm 0 +: 4
            case (P, o2, W, o1) of
                when ('0', '0', '0', '0') => __encoding aarch32_STR_r_A1_A // STR_r_A1_post
                when ('0', '0', '0', '1') => __encoding aarch32_LDR_r_A1_A // LDR_r_A1_post
                when ('0', '0', '1', '0') => __encoding aarch32_STRT_A2_A // STRT_A2
                when ('0', '0', '1', '1') => __encoding aarch32_LDRT_A2_A // LDRT_A2
                when ('0', '1', '0', '0') => __encoding aarch32_STRB_r_A1_A // STRB_r_A1_post
                when ('0', '1', '0', '1') => __encoding aarch32_LDRB_r_A1_A // LDRB_r_A1_post
                when ('0', '1', '1', '0') => __encoding aarch32_STRBT_A2_A // STRBT_A2
                when ('0', '1', '1', '1') => __encoding aarch32_LDRBT_A2_A // LDRBT_A2
                when ('1', '0', _, '0') => __encoding aarch32_STR_r_A1_A // STR_r_A1_pre
                when ('1', '0', _, '1') => __encoding aarch32_LDR_r_A1_A // LDR_r_A1_pre
                when ('1', '1', _, '0') => __encoding aarch32_STRB_r_A1_A // STRB_r_A1_pre
                when ('1', '1', _, '1') => __encoding aarch32_LDRB_r_A1_A // LDRB_r_A1_pre
        when (!'1111', '011', _, '1', _) =>
            // media
            case (28 +: 4, 25 +: 3, 20 +: 5, 8 +: 12, 5 +: 3, 4 +: 1, 0 +: 4) of
                when (_, _, '00xxx', _, _, _, _) => // parallel
                    __field cond 28 +: 4
                    __field op1 20 +: 3
                    __field Rn 16 +: 4
                    __field Rd 12 +: 4
                    __field B 7 +: 1
                    __field op2 5 +: 2
                    __field Rm 0 +: 4
                    case (op1, B, op2) of
                        when ('000', _, _) => __UNALLOCATED
                        when ('001', '0', '00') => __encoding aarch32_SADD16_A1_A // SADD16_A1
                        when ('001', '0', '01') => __encoding aarch32_SASX_A1_A // SASX_A1
                        when ('001', '0', '10') => __encoding aarch32_SSAX_A1_A // SSAX_A1
                        when ('001', '0', '11') => __encoding aarch32_SSUB16_A1_A // SSUB16_A1
                        when ('001', '1', '00') => __encoding aarch32_SADD8_A1_A // SADD8_A1
                        when ('001', '1', '01') => __UNALLOCATED
                        when ('001', '1', '10') => __UNALLOCATED
                        when ('001', '1', '11') => __encoding aarch32_SSUB8_A1_A // SSUB8_A1
                        when ('010', '0', '00') => __encoding aarch32_QADD16_A1_A // QADD16_A1
                        when ('010', '0', '01') => __encoding aarch32_QASX_A1_A // QASX_A1
                        when ('010', '0', '10') => __encoding aarch32_QSAX_A1_A // QSAX_A1
                        when ('010', '0', '11') => __encoding aarch32_QSUB16_A1_A // QSUB16_A1
                        when ('010', '1', '00') => __encoding aarch32_QADD8_A1_A // QADD8_A1
                        when ('010', '1', '01') => __UNALLOCATED
                        when ('010', '1', '10') => __UNALLOCATED
                        when ('010', '1', '11') => __encoding aarch32_QSUB8_A1_A // QSUB8_A1
                        when ('011', '0', '00') => __encoding aarch32_SHADD16_A1_A // SHADD16_A1
                        when ('011', '0', '01') => __encoding aarch32_SHASX_A1_A // SHASX_A1
                        when ('011', '0', '10') => __encoding aarch32_SHSAX_A1_A // SHSAX_A1
                        when ('011', '0', '11') => __encoding aarch32_SHSUB16_A1_A // SHSUB16_A1
                        when ('011', '1', '00') => __encoding aarch32_SHADD8_A1_A // SHADD8_A1
                        when ('011', '1', '01') => __UNALLOCATED
                        when ('011', '1', '10') => __UNALLOCATED
                        when ('011', '1', '11') => __encoding aarch32_SHSUB8_A1_A // SHSUB8_A1
                        when ('100', _, _) => __UNALLOCATED
                        when ('101', '0', '00') => __encoding aarch32_UADD16_A1_A // UADD16_A1
                        when ('101', '0', '01') => __encoding aarch32_UASX_A1_A // UASX_A1
                        when ('101', '0', '10') => __encoding aarch32_USAX_A1_A // USAX_A1
                        when ('101', '0', '11') => __encoding aarch32_USUB16_A1_A // USUB16_A1
                        when ('101', '1', '00') => __encoding aarch32_UADD8_A1_A // UADD8_A1
                        when ('101', '1', '01') => __UNALLOCATED
                        when ('101', '1', '10') => __UNALLOCATED
                        when ('101', '1', '11') => __encoding aarch32_USUB8_A1_A // USUB8_A1
                        when ('110', '0', '00') => __encoding aarch32_UQADD16_A1_A // UQADD16_A1
                        when ('110', '0', '01') => __encoding aarch32_UQASX_A1_A // UQASX_A1
                        when ('110', '0', '10') => __encoding aarch32_UQSAX_A1_A // UQSAX_A1
                        when ('110', '0', '11') => __encoding aarch32_UQSUB16_A1_A // UQSUB16_A1
                        when ('110', '1', '00') => __encoding aarch32_UQADD8_A1_A // UQADD8_A1
                        when ('110', '1', '01') => __UNALLOCATED
                        when ('110', '1', '10') => __UNALLOCATED
                        when ('110', '1', '11') => __encoding aarch32_UQSUB8_A1_A // UQSUB8_A1
                        when ('111', '0', '00') => __encoding aarch32_UHADD16_A1_A // UHADD16_A1
                        when ('111', '0', '01') => __encoding aarch32_UHASX_A1_A // UHASX_A1
                        when ('111', '0', '10') => __encoding aarch32_UHSAX_A1_A // UHSAX_A1
                        when ('111', '0', '11') => __encoding aarch32_UHSUB16_A1_A // UHSUB16_A1
                        when ('111', '1', '00') => __encoding aarch32_UHADD8_A1_A // UHADD8_A1
                        when ('111', '1', '01') => __UNALLOCATED
                        when ('111', '1', '10') => __UNALLOCATED
                        when ('111', '1', '11') => __encoding aarch32_UHSUB8_A1_A // UHSUB8_A1
                when (_, _, '01000', _, '101', _, _) => // selbytes
                    __field cond 28 +: 4
                    __field Rn 16 +: 4
                    __field Rd 12 +: 4
                    __field Rm 0 +: 4
                    case () of
                        when () => __encoding aarch32_SEL_A1_A // SEL_A1
                when (_, _, '01000', _, '001', _, _) => __UNPREDICTABLE
                when (_, _, '01000', _, 'xx0', _, _) => // pack
                    __field cond 28 +: 4
                    __field Rn 16 +: 4
                    __field Rd 12 +: 4
                    __field imm5 7 +: 5
                    __field tb 6 +: 1
                    __field Rm 0 +: 4
                    case () of
                        when () => __encoding aarch32_PKH_A1_A // PKHTB_A1
                when (_, _, '01001', _, 'x01', _, _) => __UNPREDICTABLE
                when (_, _, '01001', _, 'xx0', _, _) => __UNPREDICTABLE
                when (_, _, '0110x', _, 'x01', _, _) => __UNPREDICTABLE
                when (_, _, '0110x', _, 'xx0', _, _) => __UNPREDICTABLE
                when (_, _, '01x10', _, '001', _, _) => // sat16
                    __field cond 28 +: 4
                    __field U 22 +: 1
                    __field sat_imm 16 +: 4
                    __field Rd 12 +: 4
                    __field Rn 0 +: 4
                    case (U) of
                        when ('0') => __encoding aarch32_SSAT16_A1_A // SSAT16_A1
                        when ('1') => __encoding aarch32_USAT16_A1_A // USAT16_A1
                when (_, _, '01x10', _, '101', _, _) => __UNPREDICTABLE
                when (_, _, '01x11', _, 'x01', _, _) => // reverse
                    __field cond 28 +: 4
                    __field o1 22 +: 1
                    __field Rd 12 +: 4
                    __field o2 7 +: 1
                    __field Rm 0 +: 4
                    case (o1, o2) of
                        when ('0', '0') => __encoding aarch32_REV_A1_A // REV_A1
                        when ('0', '1') => __encoding aarch32_REV16_A1_A // REV16_A1
                        when ('1', '0') => __encoding aarch32_RBIT_A1_A // RBIT_A1
                        when ('1', '1') => __encoding aarch32_REVSH_A1_A // REVSH_A1
                when (_, _, '01x1x', _, 'xx0', _, _) => // sat32
                    __field cond 28 +: 4
                    __field U 22 +: 1
                    __field sat_imm 16 +: 5
                    __field Rd 12 +: 4
                    __field imm5 7 +: 5
                    __field sh 6 +: 1
                    __field Rn 0 +: 4
                    case (U) of
                        when ('0') => __encoding aarch32_SSAT_A1_A // SSAT_A1_ASR
                        when ('1') => __encoding aarch32_USAT_A1_A // USAT_A1_ASR
                when (_, _, '01xxx', _, '111', _, _) => __UNPREDICTABLE
                when (_, _, '01xxx', _, '011', _, _) => // extend
                    __field cond 28 +: 4
                    __field U 22 +: 1
                    __field op 20 +: 2
                    __field Rn 16 +: 4
                    __field Rd 12 +: 4
                    __field rotate 10 +: 2
                    __field Rm 0 +: 4
                    case (U, op, Rn) of
                        when ('0', '00', !'1111') => __encoding aarch32_SXTAB16_A1_A // SXTAB16_A1
                        when ('0', '00', '1111') => __encoding aarch32_SXTB16_A1_A // SXTB16_A1
                        when ('0', '10', !'1111') => __encoding aarch32_SXTAB_A1_A // SXTAB_A1
                        when ('0', '10', '1111') => __encoding aarch32_SXTB_A1_A // SXTB_A1
                        when ('0', '11', !'1111') => __encoding aarch32_SXTAH_A1_A // SXTAH_A1
                        when ('0', '11', '1111') => __encoding aarch32_SXTH_A1_A // SXTH_A1
                        when ('1', '00', !'1111') => __encoding aarch32_UXTAB16_A1_A // UXTAB16_A1
                        when ('1', '00', '1111') => __encoding aarch32_UXTB16_A1_A // UXTB16_A1
                        when ('1', '10', !'1111') => __encoding aarch32_UXTAB_A1_A // UXTAB_A1
                        when ('1', '10', '1111') => __encoding aarch32_UXTB_A1_A // UXTB_A1
                        when ('1', '11', !'1111') => __encoding aarch32_UXTAH_A1_A // UXTAH_A1
                        when ('1', '11', '1111') => __encoding aarch32_UXTH_A1_A // UXTH_A1
                when (_, _, '10xxx', _, _, _, _) => // smul_div
                    __field cond 28 +: 4
                    __field op1 20 +: 3
                    __field Rd 16 +: 4
                    __field Ra 12 +: 4
                    __field Rm 8 +: 4
                    __field op2 5 +: 3
                    __field Rn 0 +: 4
                    case (op1, Ra, op2) of
                        when ('000', !'1111', '000') => __encoding aarch32_SMLAD_A1_A // SMLAD_A1
                        when ('000', !'1111', '001') => __encoding aarch32_SMLAD_A1_A // SMLADX_A1
                        when ('000', !'1111', '010') => __encoding aarch32_SMLSD_A1_A // SMLSD_A1
                        when ('000', !'1111', '011') => __encoding aarch32_SMLSD_A1_A // SMLSDX_A1
                        when ('000', _, '1xx') => __UNALLOCATED
                        when ('000', '1111', '000') => __encoding aarch32_SMUAD_A1_A // SMUAD_A1
                        when ('000', '1111', '001') => __encoding aarch32_SMUAD_A1_A // SMUADX_A1
                        when ('000', '1111', '010') => __encoding aarch32_SMUSD_A1_A // SMUSD_A1
                        when ('000', '1111', '011') => __encoding aarch32_SMUSD_A1_A // SMUSDX_A1
                        when ('001', _, '000') => __encoding aarch32_SDIV_A1_A // SDIV_A1
                        when ('001', _, !'000') => __UNALLOCATED
                        when ('010', _, _) => __UNALLOCATED
                        when ('011', _, '000') => __encoding aarch32_UDIV_A1_A // UDIV_A1
                        when ('011', _, !'000') => __UNALLOCATED
                        when ('100', _, '000') => __encoding aarch32_SMLALD_A1_A // SMLALD_A1
                        when ('100', _, '001') => __encoding aarch32_SMLALD_A1_A // SMLALDX_A1
                        when ('100', _, '010') => __encoding aarch32_SMLSLD_A1_A // SMLSLD_A1
                        when ('100', _, '011') => __encoding aarch32_SMLSLD_A1_A // SMLSLDX_A1
                        when ('100', _, '1xx') => __UNALLOCATED
                        when ('101', !'1111', '000') => __encoding aarch32_SMMLA_A1_A // SMMLA_A1
                        when ('101', !'1111', '001') => __encoding aarch32_SMMLA_A1_A // SMMLAR_A1
                        when ('101', _, '01x') => __UNALLOCATED
                        when ('101', _, '10x') => __UNALLOCATED
                        when ('101', _, '110') => __encoding aarch32_SMMLS_A1_A // SMMLS_A1
                        when ('101', _, '111') => __encoding aarch32_SMMLS_A1_A // SMMLSR_A1
                        when ('101', '1111', '000') => __encoding aarch32_SMMUL_A1_A // SMMUL_A1
                        when ('101', '1111', '001') => __encoding aarch32_SMMUL_A1_A // SMMULR_A1
                        when ('11x', _, _) => __UNALLOCATED
                when (_, _, '11000', _, '000', _, _) => // usad
                    __field cond 28 +: 4
                    __field Rd 16 +: 4
                    __field Ra 12 +: 4
                    __field Rm 8 +: 4
                    __field Rn 0 +: 4
                    case (Ra) of
                        when (!'1111') => __encoding aarch32_USADA8_A1_A // USADA8_A1
                        when ('1111') => __encoding aarch32_USAD8_A1_A // USAD8_A1
                when (_, _, '11000', _, '100', _, _) => __UNPREDICTABLE
                when (_, _, '11001', _, 'x00', _, _) => __UNPREDICTABLE
                when (_, _, '1101x', _, 'x00', _, _) => __UNPREDICTABLE
                when (_, _, '110xx', _, '111', _, _) => __UNPREDICTABLE
                when (_, _, '1110x', _, '111', _, _) => __UNPREDICTABLE
                when (_, _, '1110x', _, 'x00', _, _) => // bfi
                    __field cond 28 +: 4
                    __field msb 16 +: 5
                    __field Rd 12 +: 4
                    __field lsb 7 +: 5
                    __field Rn 0 +: 4
                    case (Rn) of
                        when (!'1111') => __encoding aarch32_BFI_A1_A // BFI_A1
                        when ('1111') => __encoding aarch32_BFC_A1_A // BFC_A1
                when (_, _, '11110', _, '111', _, _) => __UNPREDICTABLE
                when (_, _, '11111', _, '111', _, _) => // udf
                    __field cond 28 +: 4
                    __field imm12 8 +: 12
                    __field imm4 0 +: 4
                    case (cond) of
                        when ('0xxx') => __UNALLOCATED
                        when ('10xx') => __UNALLOCATED
                        when ('110x') => __UNALLOCATED
                        when ('1110') => __encoding aarch32_UDF_A1_A // UDF_A1
                when (_, _, '1111x', _, 'x00', _, _) => __UNPREDICTABLE
                when (_, _, '11x0x', _, 'x10', _, _) => __UNPREDICTABLE
                when (_, _, '11x1x', _, 'x10', _, _) => // bfx
                    __field cond 28 +: 4
                    __field U 22 +: 1
                    __field widthm1 16 +: 5
                    __field Rd 12 +: 4
                    __field lsb 7 +: 5
                    __field Rn 0 +: 4
                    case (U) of
                        when ('0') => __encoding aarch32_SBFX_A1_A // SBFX_A1
                        when ('1') => __encoding aarch32_UBFX_A1_A // UBFX_A1
                when (_, _, '11xxx', _, '011', _, _) => __UNPREDICTABLE
                when (_, _, '11xxx', _, 'x01', _, _) => __UNPREDICTABLE
        when (_, '10x', _, _, _) =>
            // brblk
            case (28 +: 4, 26 +: 2, 25 +: 1, 0 +: 25) of
                when ('1111', _, '0', _) => // ldstexcept
                    __field P 24 +: 1
                    __field U 23 +: 1
                    __field S 22 +: 1
                    __field W 21 +: 1
                    __field L 20 +: 1
                    __field Rn 16 +: 4
                    __field op 5 +: 11
                    __field mode 0 +: 5
                    case (P, U, S, L) of
                        when (_, _, '0', '0') => __UNALLOCATED
                        when ('0', '0', '0', '1') => __encoding aarch32_RFE_A1_AS // RFEDA_A1_AS
                        when ('0', '0', '1', '0') => __encoding aarch32_SRS_A1_AS // SRSDA_A1_AS
                        when ('0', '1', '0', '1') => __encoding aarch32_RFE_A1_AS // RFEIA_A1_AS
                        when ('0', '1', '1', '0') => __encoding aarch32_SRS_A1_AS // SRSIA_A1_AS
                        when ('1', '0', '0', '1') => __encoding aarch32_RFE_A1_AS // RFEDB_A1_AS
                        when ('1', '0', '1', '0') => __encoding aarch32_SRS_A1_AS // SRSDB_A1_AS
                        when (_, _, '1', '1') => __UNALLOCATED
                        when ('1', '1', '0', '1') => __encoding aarch32_RFE_A1_AS // RFEIB_A1_AS
                        when ('1', '1', '1', '0') => __encoding aarch32_SRS_A1_AS // SRSIB_A1_AS
                when (!'1111', _, '0', _) => // ldstm
                    __field cond 28 +: 4
                    __field P 24 +: 1
                    __field U 23 +: 1
                    __field op 22 +: 1
                    __field W 21 +: 1
                    __field L 20 +: 1
                    __field Rn 16 +: 4
                    __field register_list 0 +: 16
                    case (P, U, op, L, register_list) of
                        when ('0', '0', '0', '0', _) => __encoding aarch32_STMDA_A1_A // STMDA_A1
                        when ('0', '0', '0', '1', _) => __encoding aarch32_LDMDA_A1_A // LDMDA_A1
                        when ('0', '1', '0', '0', _) => __encoding aarch32_STM_A1_A // STM_A1
                        when ('0', '1', '0', '1', _) => __encoding aarch32_LDM_A1_A // LDM_A1
                        when (_, _, '1', '0', _) => __encoding aarch32_STM_u_A1_AS // STM_u_A1_AS
                        when ('1', '0', '0', '0', _) => __encoding aarch32_STMDB_A1_A // STMDB_A1
                        when ('1', '0', '0', '1', _) => __encoding aarch32_LDMDB_A1_A // LDMDB_A1
                        when (_, _, '1', '1', '0xxxxxxxxxxxxxxx') => __encoding aarch32_LDM_u_A1_AS // LDM_u_A1_AS
                        when ('1', '1', '0', '0', _) => __encoding aarch32_STMIB_A1_A // STMIB_A1
                        when ('1', '1', '0', '1', _) => __encoding aarch32_LDMIB_A1_A // LDMIB_A1
                        when (_, _, '1', '1', '1xxxxxxxxxxxxxxx') => __encoding aarch32_LDM_e_A1_AS // LDM_e_A1_AS
                when (_, _, '1', _) => // b_imm
                    __field cond 28 +: 4
                    __field H 24 +: 1
                    __field imm24 0 +: 24
                    case (cond, H) of
                        when (!'1111', '0') => __encoding aarch32_B_A1_A // B_A1
                        when (!'1111', '1') => __encoding aarch32_BL_i_A1_A
                        when ('1111', _) => __encoding aarch32_BL_i_A1_A
        when (_, '11x', _, _, _) =>
            // cops_as
            case (28 +: 4, 26 +: 2, 24 +: 2, 12 +: 12, 9 +: 3, 5 +: 4, 4 +: 1, 0 +: 4) of
                when (_, _, '0x', _, '111', _, _, _) =>
                    // sysldst_mov64
                    case (28 +: 4, 25 +: 3, 21 +: 4, 12 +: 9, 9 +: 3, 0 +: 9) of
                        when (_, _, '00x0', _, _, _) => // movcpgp64
                            __field cond 28 +: 4
                            __field D 22 +: 1
                            __field L 20 +: 1
                            __field Rt2 16 +: 4
                            __field Rt 12 +: 4
                            __field cp15 8 +: 1
                            __field opc1 4 +: 4
                            __field CRm 0 +: 4
                            case (cond, D, L) of
                                when (!'1111', '1', '0') => __encoding aarch32_MCRR_T1A1_A // MCRR_A1
                                when (!'1111', '1', '1') => __encoding aarch32_MRRC_T1A1_A // MRRC_A1
                                when (_, '0', _) => __UNALLOCATED
                                when ('1111', '1', _) => __UNALLOCATED
                        when (_, _, !'00x0', _, _, _) => // ldstcp
                            __field cond 28 +: 4
                            __field P 24 +: 1
                            __field U 23 +: 1
                            __field D 22 +: 1
                            __field W 21 +: 1
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field CRd 12 +: 4
                            __field cp15 8 +: 1
                            __field imm8 0 +: 8
                            case (cond, P:U:W, D, L, Rn, CRd, cp15) of
                                when (!'1111', !'000', '0', _, _, !'0101', '0') => __UNALLOCATED
                                when (!'1111', !'000', '0', '1', '1111', '0101', '0') => __encoding aarch32_LDC_l_T1A1_A // LDC_l_A1
                                when (!'1111', !'000', _, _, _, _, '1') => __UNALLOCATED
                                when (!'1111', !'000', '1', _, _, '0101', '0') => __UNALLOCATED
                                when (!'1111', '0x1', '0', '0', _, '0101', '0') => __encoding aarch32_STC_T1A1_A // STC_A1_post
                                when (!'1111', '0x1', '0', '1', !'1111', '0101', '0') => __encoding aarch32_LDC_i_T1A1_A // LDC_i_A1_post
                                when (!'1111', '010', '0', '0', _, '0101', '0') => __encoding aarch32_STC_T1A1_A // STC_A1_unind
                                when (!'1111', '010', '0', '1', !'1111', '0101', '0') => __encoding aarch32_LDC_i_T1A1_A // LDC_i_A1_unind
                                when (!'1111', '1x0', '0', '0', _, '0101', '0') => __encoding aarch32_STC_T1A1_A // STC_A1_off
                                when (!'1111', '1x0', '0', '1', !'1111', '0101', '0') => __encoding aarch32_LDC_i_T1A1_A // LDC_i_A1_off
                                when (!'1111', '1x1', '0', '0', _, '0101', '0') => __encoding aarch32_STC_T1A1_A // STC_A1_pre
                                when (!'1111', '1x1', '0', '1', !'1111', '0101', '0') => __encoding aarch32_LDC_i_T1A1_A // LDC_i_A1_pre
                                when ('1111', !'000', _, _, _, _, _) => __UNALLOCATED
                when (_, _, '10', _, '10x', _, '0', _) =>
                    // fpdp
                    case (28 +: 4, 24 +: 4, 20 +: 4, 16 +: 4, 12 +: 4, 10 +: 2, 8 +: 2, 7 +: 1, 6 +: 1, 5 +: 1, 4 +: 1, 0 +: 4) of
                        when ('1111', _, '0xxx', _, _, _, !'00', _, '0', _, _, _) => // fpcsel
                            __field D 22 +: 1
                            __field cc 20 +: 2
                            __field Vn 16 +: 4
                            __field Vd 12 +: 4
                            __field size 8 +: 2
                            __field N 7 +: 1
                            __field M 5 +: 1
                            __field Vm 0 +: 4
                            case (cc, size) of
                                when ('00', _) => __encoding aarch32_VSEL_A1_A // VSELEQ_A1_D
                                when ('01', _) => __encoding aarch32_VSEL_A1_A // VSELVS_A1_D
                                when (_, '01') => __UNALLOCATED
                                when ('10', _) => __encoding aarch32_VSEL_A1_A // VSELGE_A1_D
                                when ('11', _) => __encoding aarch32_VSEL_A1_A // VSELGT_A1_D
                        when ('1111', _, '1x00', _, _, _, !'00', _, _, _, _, _) => // fpminmaxnm
                            __field D 22 +: 1
                            __field Vn 16 +: 4
                            __field Vd 12 +: 4
                            __field size 8 +: 2
                            __field N 7 +: 1
                            __field op 6 +: 1
                            __field M 5 +: 1
                            __field Vm 0 +: 4
                            case (size, op) of
                                when (_, '0') => __encoding aarch32_VMAXNM_A2_A // VMAXNM_A2_D
                                when ('01', _) => __UNALLOCATED
                                when (_, '1') => __encoding aarch32_VMAXNM_A2_A // VMINNM_A2_D
                        when ('1111', _, '1x11', '0000', _, _, !'00', _, '1', _, _, _) => // fpextins
                            __field D 22 +: 1
                            __field Vd 12 +: 4
                            __field size 8 +: 2
                            __field op 7 +: 1
                            __field M 5 +: 1
                            __field Vm 0 +: 4
                            case (size, op) of
                                when ('01', _) => __UNALLOCATED
                                when ('10', '0') => __encoding aarch32_VMOVX_A1_A // VMOVX_A1
                                when ('10', '1') => __encoding aarch32_VINS_A1_A // VINS_A1
                                when ('11', _) => __UNALLOCATED
                        when ('1111', _, '1x11', '1xxx', _, _, !'00', _, '1', _, _, _) => // fpcvtrnd
                            __field D 22 +: 1
                            __field o1 18 +: 1
                            __field RM 16 +: 2
                            __field Vd 12 +: 4
                            __field size 8 +: 2
                            __field op 7 +: 1
                            __field M 5 +: 1
                            __field Vm 0 +: 4
                            case (o1, RM, size) of
                                when ('0', '00', _) => __encoding aarch32_VRINTA_vfp_A1_A // VRINTA_vfp_A1_D
                                when ('0', '01', _) => __encoding aarch32_VRINTA_vfp_A1_A // VRINTN_vfp_A1_D
                                when (_, _, '01') => __UNALLOCATED
                                when ('0', '10', _) => __encoding aarch32_VRINTA_vfp_A1_A // VRINTP_vfp_A1_D
                                when ('0', '11', _) => __encoding aarch32_VRINTA_vfp_A1_A // VRINTM_vfp_A1_D
                                when ('1', '00', _) => __encoding aarch32_VCVTA_vfp_A1_A // VCVTA_vfp_A1_D
                                when ('1', '01', _) => __encoding aarch32_VCVTA_vfp_A1_A // VCVTN_vfp_A1_D
                                when ('1', '10', _) => __encoding aarch32_VCVTA_vfp_A1_A // VCVTP_vfp_A1_D
                                when ('1', '11', _) => __encoding aarch32_VCVTA_vfp_A1_A // VCVTM_vfp_A1_D
                        when (!'1111', _, '1x11', _, _, _, _, _, '1', _, _, _) => // fpdp2reg
                            __field cond 28 +: 4
                            __field D 22 +: 1
                            __field o1 19 +: 1
                            __field opc2 16 +: 3
                            __field Vd 12 +: 4
                            __field size 8 +: 2
                            __field o3 7 +: 1
                            __field M 5 +: 1
                            __field Vm 0 +: 4
                            case (o1, opc2, size, o3) of
                                when (_, _, '00', _) => __UNALLOCATED
                                when ('0', '000', '01', '0') => __UNALLOCATED
                                when ('0', '000', _, '1') => __encoding aarch32_VABS_A2_A // VABS_A2_D
                                when ('0', '000', '10', '0') => __encoding aarch32_VMOV_r_T2A2_A // VMOV_r_A2_S
                                when ('0', '000', '11', '0') => __encoding aarch32_VMOV_r_T2A2_A // VMOV_r_A2_D
                                when ('0', '001', _, '0') => __encoding aarch32_VNEG_A2_A // VNEG_A2_D
                                when ('0', '001', _, '1') => __encoding aarch32_VSQRT_A1_A // VSQRT_A1_D
                                when ('0', '01x', '01', _) => __UNALLOCATED
                                when ('0', '010', _, '0') => __encoding aarch32_VCVTB_T1A1_A // VCVTB_A1_DH
                                when ('0', '010', _, '1') => __encoding aarch32_VCVTB_T1A1_A // VCVTT_A1_DH
                                when ('0', '011', _, '0') => __encoding aarch32_VCVTB_T1A1_A // VCVTB_A1_HD
                                when ('0', '011', _, '1') => __encoding aarch32_VCVTB_T1A1_A // VCVTT_A1_HD
                                when ('0', '100', _, '0') => __encoding aarch32_VCMP_A1_A
                                when ('0', '100', _, '1') => __encoding aarch32_VCMP_A1_A
                                when ('0', '101', _, '0') => __encoding aarch32_VCMP_A1_A
                                when ('0', '101', _, '1') => __encoding aarch32_VCMP_A1_A
                                when ('0', '110', _, '0') => __encoding aarch32_VRINTZ_vfp_A1_A // VRINTR_vfp_A1_D
                                when ('0', '110', _, '1') => __encoding aarch32_VRINTZ_vfp_A1_A // VRINTZ_vfp_A1_D
                                when ('0', '111', _, '0') => __encoding aarch32_VRINTX_vfp_A1_A // VRINTX_vfp_A1_D
                                when ('0', '111', '01', '1') => __UNALLOCATED
                                when ('0', '111', '10', '1') => __encoding aarch32_VCVT_ds_T1A1_A // VCVT_ds_A1
                                when ('0', '111', '11', '1') => __encoding aarch32_VCVT_ds_T1A1_A // VCVT_sd_A1
                                when ('1', '000', _, _) => __encoding aarch32_VCVT_iv_A1_A // VCVT_vi_A1_D
                                when ('1', '001', '01', _) => __UNALLOCATED
                                when ('1', '001', '10', _) => __UNALLOCATED
                                when ('1', '001', '11', '0') => __UNALLOCATED
                                when ('1', '001', '11', '1') => __encoding aarch32_VJCVT_A1_A // VJCVT_A1
                                when ('1', '01x', _, _) => __encoding aarch32_VCVT_xv_A1_A // VCVT_toxv_A1_D
                                when ('1', '100', _, '0') => __encoding aarch32_VCVT_iv_A1_A // VCVTR_uiv_A1_D
                                when ('1', '100', _, '1') => __encoding aarch32_VCVT_iv_A1_A // VCVT_uiv_A1_D
                                when ('1', '101', _, '0') => __encoding aarch32_VCVT_iv_A1_A // VCVTR_siv_A1_D
                                when ('1', '101', _, '1') => __encoding aarch32_VCVT_iv_A1_A // VCVT_siv_A1_D
                                when ('1', '11x', _, _) => __encoding aarch32_VCVT_xv_A1_A // VCVT_xv_A1_D
                        when (!'1111', _, '1x11', _, _, _, _, _, '0', _, _, _) => // fpimm
                            __field cond 28 +: 4
                            __field D 22 +: 1
                            __field imm4H 16 +: 4
                            __field Vd 12 +: 4
                            __field size 8 +: 2
                            __field imm4L 0 +: 4
                            case (size) of
                                when ('00') => __UNALLOCATED
                                when ('01') => __encoding aarch32_VMOV_i_A2_A // VMOV_i_A2_H
                                when ('10') => __encoding aarch32_VMOV_i_A2_A // VMOV_i_A2_S
                                when ('11') => __encoding aarch32_VMOV_i_A2_A // VMOV_i_A2_D
                        when (!'1111', _, !'1x11', _, _, _, _, _, _, _, _, _) => // fpdp3reg
                            __field cond 28 +: 4
                            __field o0 23 +: 1
                            __field D 22 +: 1
                            __field o1 20 +: 2
                            __field Vn 16 +: 4
                            __field Vd 12 +: 4
                            __field size 8 +: 2
                            __field N 7 +: 1
                            __field o2 6 +: 1
                            __field M 5 +: 1
                            __field Vm 0 +: 4
                            case (o0:o1, size, o2) of
                                when (!'111', '00', _) => __UNALLOCATED
                                when ('000', _, '0') => __encoding aarch32_VMLA_f_A2_A // VMLA_f_A2_D
                                when ('000', _, '1') => __encoding aarch32_VMLA_f_A2_A // VMLS_f_A2_D
                                when ('001', _, '0') => __encoding aarch32_VNMLA_A1_A // VNMLS_A1_D
                                when ('001', _, '1') => __encoding aarch32_VNMLA_A1_A // VNMLA_A1_D
                                when ('010', _, '0') => __encoding aarch32_VMUL_f_A2_A // VMUL_f_A2_D
                                when ('010', _, '1') => __encoding aarch32_VNMLA_A2_A // VNMUL_A1_D
                                when ('011', _, '0') => __encoding aarch32_VADD_f_A2_A // VADD_f_A2_D
                                when ('011', _, '1') => __encoding aarch32_VSUB_f_A2_A // VSUB_f_A2_D
                                when ('100', _, '0') => __encoding aarch32_VDIV_A1_A // VDIV_A1_D
                                when ('101', _, '0') => __encoding aarch32_VFNMA_A1_A // VFNMS_A1_D
                                when ('101', _, '1') => __encoding aarch32_VFNMA_A1_A // VFNMA_A1_D
                                when ('110', _, '0') => __encoding aarch32_VFMA_A2_A // VFMA_A2_D
                                when ('110', _, '1') => __encoding aarch32_VFMA_A2_A // VFMS_A2_D
                when (_, _, '10', _, '111', _, '1', _) => // movcpgp32
                    __field cond 28 +: 4
                    __field opc1 21 +: 3
                    __field L 20 +: 1
                    __field CRn 16 +: 4
                    __field Rt 12 +: 4
                    __field cp15 8 +: 1
                    __field opc2 5 +: 3
                    __field CRm 0 +: 4
                    case (cond, L) of
                        when (!'1111', '0') => __encoding aarch32_MCR_T1A1_A // MCR_A1
                        when (!'1111', '1') => __encoding aarch32_MRC_T1A1_A // MRC_A1
                        when ('1111', _) => __UNALLOCATED
                when (_, _, '11', _, _, _, _, _) =>
                    // svcall
                    case (28 +: 4, 24 +: 4, 0 +: 24) of
                        when ('1111', _, _) => __UNPREDICTABLE
                        when (!'1111', _, _) => // svc
                            __field cond 28 +: 4
                            __field imm24 0 +: 24
                            case () of
                                when () => __encoding aarch32_SVC_A1_A // SVC_A1
                when ('1111', _, '0x', _, '1x0', _, _, _) => // simd3reg_sameext
                    __field op1 23 +: 2
                    __field D 22 +: 1
                    __field op2 20 +: 2
                    __field Vn 16 +: 4
                    __field Vd 12 +: 4
                    __field op3 10 +: 1
                    __field op4 8 +: 1
                    __field N 7 +: 1
                    __field Q 6 +: 1
                    __field M 5 +: 1
                    __field U 4 +: 1
                    __field Vm 0 +: 4
                    case (op1, op2, op3, op4, Q, U) of
                        when ('x1', '0x', '0', '0', _, '0') => __encoding aarch32_VCADD_A1_A // VCADD_A1_Q
                        when ('00', '10', '0', '0', _, '1') => __encoding aarch32_VFMAL_A1_A // VFMAL_A1_Q
                        when ('00', '10', '1', '1', '0', '0') => __encoding aarch32_VDOT_A1_A // VSDOT_A1_D
                        when ('00', '10', '1', '1', '0', '1') => __encoding aarch32_VDOT_A1_A // VUDOT_A1_D
                        when ('00', '10', '1', '1', '1', '0') => __encoding aarch32_VDOT_A1_A // VSDOT_A1_Q
                        when ('00', '10', '1', '1', '1', '1') => __encoding aarch32_VDOT_A1_A // VUDOT_A1_Q
                        when ('01', '10', '0', '0', _, '1') => __encoding aarch32_VFMAL_A1_A // VFMSL_A1_Q
                        when (_, '1x', '0', '0', _, '0') => __encoding aarch32_VCMLA_A1_A // VCMLA_A1_Q
                when ('1111', _, '10', _, '1x0', _, _, _) => // simd2reg_scalarext
                    __field op1 23 +: 1
                    __field D 22 +: 1
                    __field op2 20 +: 2
                    __field Vn 16 +: 4
                    __field Vd 12 +: 4
                    __field op3 10 +: 1
                    __field op4 8 +: 1
                    __field N 7 +: 1
                    __field Q 6 +: 1
                    __field M 5 +: 1
                    __field U 4 +: 1
                    __field Vm 0 +: 4
                    case (op1, op2, op3, op4, Q, U) of
                        when ('0', _, '0', '0', _, '0') => __encoding aarch32_VCMLA_idx_A1_A // VCMLA_s_A1_QH
                        when ('0', '00', '0', '0', _, '1') => __encoding aarch32_VFMAL_i_A1_A // VFMAL_s_A1_Q
                        when ('0', '01', '0', '0', _, '1') => __encoding aarch32_VFMAL_i_A1_A // VFMSL_s_A1_Q
                        when ('0', '10', '1', '1', '0', '0') => __encoding aarch32_VDOT_s_A1_A // VSDOT_s_A1_D
                        when ('0', '10', '1', '1', '0', '1') => __encoding aarch32_VDOT_s_A1_A // VUDOT_s_A1_D
                        when ('0', '10', '1', '1', '1', '0') => __encoding aarch32_VDOT_s_A1_A // VSDOT_s_A1_Q
                        when ('0', '10', '1', '1', '1', '1') => __encoding aarch32_VDOT_s_A1_A // VUDOT_s_A1_Q
                        when ('1', _, '0', '0', _, '0') => __encoding aarch32_VCMLA_idx_A1_A // VCMLA_s_A1_QS
                when (!'1111', _, '0x', _, '10x', _, _, _) =>
                    // simdldst_mov64
                    case (28 +: 4, 25 +: 3, 21 +: 4, 12 +: 9, 10 +: 2, 0 +: 10) of
                        when (_, _, '00x0', _, _, _) => // movsimdfpgp64
                            __field cond 28 +: 4
                            __field D 22 +: 1
                            __field op 20 +: 1
                            __field Rt2 16 +: 4
                            __field Rt 12 +: 4
                            __field size 8 +: 2
                            __field opc2 6 +: 2
                            __field M 5 +: 1
                            __field o3 4 +: 1
                            __field Vm 0 +: 4
                            case (D, op, size, opc2, o3) of
                                when ('0', _, _, _, _) => __UNALLOCATED
                                when ('1', _, _, _, '0') => __UNALLOCATED
                                when ('1', _, '0x', '00', '1') => __UNALLOCATED
                                when ('1', _, _, '01', _) => __UNALLOCATED
                                when ('1', '0', '10', '00', '1') => __encoding aarch32_VMOV_ss_T1A1_A // VMOV_toss_A1
                                when ('1', '0', '11', '00', '1') => __encoding aarch32_VMOV_d_T1A1_A // VMOV_tod_A1
                                when ('1', _, _, '1x', _) => __UNALLOCATED
                                when ('1', '1', '10', '00', '1') => __encoding aarch32_VMOV_ss_T1A1_A // VMOV_ss_A1
                                when ('1', '1', '11', '00', '1') => __encoding aarch32_VMOV_d_T1A1_A // VMOV_d_A1
                        when (_, _, !'00x0', _, _, _) => // ldstsimdfp
                            __field cond 28 +: 4
                            __field P 24 +: 1
                            __field U 23 +: 1
                            __field D 22 +: 1
                            __field W 21 +: 1
                            __field L 20 +: 1
                            __field Rn 16 +: 4
                            __field Vd 12 +: 4
                            __field size 8 +: 2
                            __field imm8 0 +: 8
                            case (P, U, W, L, Rn, size, imm8) of
                                when ('0', '0', '1', _, _, _, _) => __UNALLOCATED
                                when ('0', '1', _, _, _, '0x', _) => __UNALLOCATED
                                when ('0', '1', _, '0', _, '10', _) => __encoding aarch32_VSTM_T2A2_A // VSTM_A2
                                when ('0', '1', _, '0', _, '11', 'xxxxxxx0') => __encoding aarch32_VSTM_T1A1_A // VSTM_A1
                                when ('0', '1', _, '0', _, '11', 'xxxxxxx1') => __encoding aarch32_VSTM_T1A1_A // FSTMIAX_A1
                                when ('0', '1', _, '1', _, '10', _) => __encoding aarch32_VLDM_T2A2_A // VLDM_A2
                                when ('0', '1', _, '1', _, '11', 'xxxxxxx0') => __encoding aarch32_VLDM_T1A1_A // VLDM_A1
                                when ('0', '1', _, '1', _, '11', 'xxxxxxx1') => __encoding aarch32_VLDM_T1A1_A // FLDMIAX_A1
                                when ('1', _, '0', '0', _, _, _) => __encoding aarch32_VSTR_A1_A // VSTR_A1_D
                                when ('1', _, '0', _, _, '00', _) => __UNALLOCATED
                                when ('1', _, '0', '1', !'1111', _, _) => __encoding aarch32_VLDR_A1_A // VLDR_A1_D
                                when ('1', '0', '1', _, _, '0x', _) => __UNALLOCATED
                                when ('1', '0', '1', '0', _, '10', _) => __encoding aarch32_VSTM_T2A2_A // VSTMDB_A2
                                when ('1', '0', '1', '0', _, '11', 'xxxxxxx0') => __encoding aarch32_VSTM_T1A1_A // VSTMDB_A1
                                when ('1', '0', '1', '0', _, '11', 'xxxxxxx1') => __encoding aarch32_VSTM_T1A1_A // FSTMDBX_A1
                                when ('1', '0', '1', '1', _, '10', _) => __encoding aarch32_VLDM_T2A2_A // VLDMDB_A2
                                when ('1', '0', '1', '1', _, '11', 'xxxxxxx0') => __encoding aarch32_VLDM_T1A1_A // VLDMDB_A1
                                when ('1', '0', '1', '1', _, '11', 'xxxxxxx1') => __encoding aarch32_VLDM_T1A1_A // FLDMDBX_A1
                                when ('1', _, '0', '1', '1111', _, _) => __encoding aarch32_VLDR_A1_A // VLDR_l_A1_D
                                when ('1', '1', '1', _, _, _, _) => __UNALLOCATED
                when (!'1111', _, '10', _, '10x', _, '1', _) =>
                    // fpsimd_mov32
                    case (28 +: 4, 24 +: 4, 21 +: 3, 12 +: 9, 9 +: 3, 8 +: 1, 5 +: 3, 0 +: 5) of
                        when (_, _, '000', _, _, '0', _, _) => // movfpgp32
                            __field cond 28 +: 4
                            __field op 20 +: 1
                            __field Vn 16 +: 4
                            __field Rt 12 +: 4
                            __field N 7 +: 1
                            case () of
                                when () => __encoding aarch32_VMOV_s_A1_A // VMOV_s_A1
                        when (_, _, '111', _, _, '0', _, _) => // movfpsr
                            __field cond 28 +: 4
                            __field L 20 +: 1
                            __field reg 16 +: 4
                            __field Rt 12 +: 4
                            case (L) of
                                when ('0') => __encoding aarch32_VMSR_T1A1_AS // VMSR_A1_AS
                                when ('1') => __encoding aarch32_VMRS_T1A1_AS // VMRS_A1_AS
                        when (_, _, _, _, _, '1', _, _) => // movsimdgp
                            __field cond 28 +: 4
                            __field opc1 21 +: 3
                            __field L 20 +: 1
                            __field Vn 16 +: 4
                            __field Rt 12 +: 4
                            __field N 7 +: 1
                            __field opc2 5 +: 2
                            case (opc1, L, opc2) of
                                when ('0xx', '0', _) => __encoding aarch32_VMOV_rs_T1A1_A // VMOV_rs_A1
                                when (_, '1', _) => __encoding aarch32_VMOV_sr_T1A1_A // VMOV_sr_A1
                                when ('1xx', '0', '0x') => __encoding aarch32_VDUP_r_T1A1_A // VDUP_r_A1
                                when ('1xx', '0', '1x') => __UNALLOCATED
        when ('1111', '0xx', _, _, _) =>
            // uncond_as
            case (27 +: 5, 25 +: 2, 21 +: 4, 20 +: 1, 0 +: 20) of
                when (_, '00', _, _, _) =>
                    // uncondmisc
                    case (25 +: 7, 20 +: 5, 8 +: 12, 4 +: 4, 0 +: 4) of
                        when (_, '0xxxx', _, _, _) => __UNPREDICTABLE
                        when (_, '10000', _, 'xx0x', _) => // cps
                            __field imod 18 +: 2
                            __field M 17 +: 1
                            __field op 16 +: 1
                            __field E 9 +: 1
                            __field A 8 +: 1
                            __field I 7 +: 1
                            __field F 6 +: 1
                            __field mode 0 +: 5
                            case (imod, M, op, mode) of
                                when (_, _, '1', '0xxxx') => __encoding aarch32_SETEND_A1_A // SETEND_A1
                                when (_, _, '0', _) => __encoding aarch32_CPS_A1_AS // CPSID_A1_ASM
                                when (_, _, '1', '1xxxx') => __UNALLOCATED
                        when (_, '10001', _, '1000', _) => __UNPREDICTABLE
                        when (_, '10001', _, 'x100', _) => __UNPREDICTABLE
                        when (_, '10001', _, 'xx01', _) => __UNPREDICTABLE
                        when (_, '10001', _, '0000', _) => // setpan
                            __field imm1 9 +: 1
                            case () of
                                when () => __encoding aarch32_SETPAN_A1_A // SETPAN_A1
                        when (_, '1000x', _, '0111', _) => __UNPREDICTABLE
                        when (_, '10010', _, '0111', _) => __UNALLOCATED
                        when (_, '10011', _, '0111', _) => __UNPREDICTABLE
                        when (_, '1001x', _, 'xx0x', _) => __UNPREDICTABLE
                        when (_, '100xx', _, '0011', _) => __UNPREDICTABLE
                        when (_, '100xx', _, '0x10', _) => __UNPREDICTABLE
                        when (_, '100xx', _, '1x1x', _) => __UNPREDICTABLE
                        when (_, '101xx', _, _, _) => __UNPREDICTABLE
                        when (_, '11xxx', _, _, _) => __UNPREDICTABLE
                when (_, '01', _, _, _) =>
                    // advsimddp
                    case (25 +: 7, 24 +: 1, 23 +: 1, 5 +: 18, 4 +: 1, 0 +: 4) of
                        when (_, _, '0', _, _, _) => // simd3reg_same
                            __field U 24 +: 1
                            __field D 22 +: 1
                            __field size 20 +: 2
                            __field Vn 16 +: 4
                            __field Vd 12 +: 4
                            __field opc 8 +: 4
                            __field N 7 +: 1
                            __field Q 6 +: 1
                            __field M 5 +: 1
                            __field o1 4 +: 1
                            __field Vm 0 +: 4
                            case (U, size, opc, Q, o1) of
                                when ('0', '0x', '1100', _, '1') => __encoding aarch32_VFMA_A1_A // VFMA_A1_Q
                                when ('0', '0x', '1101', _, '0') => __encoding aarch32_VADD_f_A1_A // VADD_f_A1_Q
                                when ('0', '0x', '1101', _, '1') => __encoding aarch32_VMLA_f_A1_A // VMLA_f_A1_Q
                                when ('0', '0x', '1110', _, '0') => __encoding aarch32_VCEQ_r_T1A1_A
                                when ('0', '0x', '1111', _, '0') => __encoding aarch32_VMAX_f_A1_A // VMAX_f_A1_Q
                                when ('0', '0x', '1111', _, '1') => __encoding aarch32_VRECPS_A1_A // VRECPS_A1_Q
                                when (_, _, '0000', _, '0') => __encoding aarch32_VHADD_T1A1_A // VHADD_A1_Q
                                when ('0', '00', '0001', _, '1') => __encoding aarch32_VAND_r_T1A1_A // VAND_r_A1_Q
                                when (_, _, '0000', _, '1') => __encoding aarch32_VQADD_T1A1_A // VQADD_A1_Q
                                when (_, _, '0001', _, '0') => __encoding aarch32_VRHADD_T1A1_A // VRHADD_A1_Q
                                when ('0', '00', '1100', _, '0') => __encoding aarch32_SHA1C_A1_A // SHA1C_A1
                                when (_, _, '0010', _, '0') => __encoding aarch32_VHADD_T1A1_A // VHSUB_A1_Q
                                when ('0', '01', '0001', _, '1') => __encoding aarch32_VBIC_r_T1A1_A // VBIC_r_A1_Q
                                when (_, _, '0010', _, '1') => __encoding aarch32_VQSUB_T1A1_A // VQSUB_A1_Q
                                when (_, _, '0011', _, '0') => __encoding aarch32_VCGT_r_T1A1_A
                                when (_, _, '0011', _, '1') => __encoding aarch32_VCGE_r_T1A1_A
                                when ('0', '01', '1100', _, '0') => __encoding aarch32_SHA1P_A1_A // SHA1P_A1
                                when ('0', '1x', '1100', _, '1') => __encoding aarch32_VFMA_A1_A // VFMS_A1_Q
                                when ('0', '1x', '1101', _, '0') => __encoding aarch32_VSUB_f_A1_A // VSUB_f_A1_Q
                                when ('0', '1x', '1101', _, '1') => __encoding aarch32_VMLA_f_A1_A // VMLS_f_A1_Q
                                when ('0', '1x', '1110', _, '0') => __UNALLOCATED
                                when ('0', '1x', '1111', _, '0') => __encoding aarch32_VMAX_f_A1_A // VMIN_f_A1_Q
                                when ('0', '1x', '1111', _, '1') => __encoding aarch32_VRSQRTS_A1_A // VRSQRTS_A1_Q
                                when (_, _, '0100', _, '0') => __encoding aarch32_VSHL_r_T1A1_A // VSHL_r_A1_Q
                                when ('0', _, '1000', _, '0') => __encoding aarch32_VADD_i_T1A1_A // VADD_i_A1_Q
                                when ('0', '10', '0001', _, '1') => __encoding aarch32_VORR_r_T1A1_A // VORR_r_A1_Q
                                when ('0', _, '1000', _, '1') => __encoding aarch32_VTST_T1A1_A // VTST_A1_Q
                                when (_, _, '0100', _, '1') => __encoding aarch32_VQSHL_r_T1A1_A // VQSHL_r_A1_Q
                                when ('0', _, '1001', _, '0') => __encoding aarch32_VMLA_i_T1A1_A // VMLA_i_A1_Q
                                when (_, _, '0101', _, '0') => __encoding aarch32_VRSHL_T1A1_A // VRSHL_A1_Q
                                when (_, _, '0101', _, '1') => __encoding aarch32_VQRSHL_T1A1_A // VQRSHL_A1_Q
                                when ('0', _, '1011', _, '0') => __encoding aarch32_VQDMULH_T1A1_A // VQDMULH_A1_Q
                                when ('0', '10', '1100', _, '0') => __encoding aarch32_SHA1M_A1_A // SHA1M_A1
                                when ('0', _, '1011', _, '1') => __encoding aarch32_VPADD_i_T1A1_A // VPADD_i_A1
                                when (_, _, '0110', _, '0') => __encoding aarch32_VMAX_i_T1A1_A // VMAX_i_A1_Q
                                when ('0', '11', '0001', _, '1') => __encoding aarch32_VORN_r_T1A1_A // VORN_r_A1_Q
                                when (_, _, '0110', _, '1') => __encoding aarch32_VMAX_i_T1A1_A // VMIN_i_A1_Q
                                when (_, _, '0111', _, '0') => __encoding aarch32_VABD_i_T1A1_A // VABD_i_A1_Q
                                when (_, _, '0111', _, '1') => __encoding aarch32_VABA_T1A1_A // VABA_A1_Q
                                when ('0', '11', '1100', _, '0') => __encoding aarch32_SHA1SU0_A1_A // SHA1SU0_A1
                                when ('1', '0x', '1101', _, '0') => __encoding aarch32_VPADD_f_A1_A // VPADD_f_A1
                                when ('1', '0x', '1101', _, '1') => __encoding aarch32_VMUL_f_A1_A // VMUL_f_A1_Q
                                when ('1', '0x', '1110', _, '0') => __encoding aarch32_VCGE_r_T1A1_A
                                when ('1', '0x', '1110', _, '1') => __encoding aarch32_VACGE_A1_A // VACGE_A1_Q
                                when ('1', '0x', '1111', '0', '0') => __encoding aarch32_VPMAX_f_A1_A // VPMAX_f_A1
                                when ('1', '0x', '1111', _, '1') => __encoding aarch32_VMAXNM_A1_A // VMAXNM_A1_Q
                                when ('1', '00', '0001', _, '1') => __encoding aarch32_VEOR_T1A1_A // VEOR_A1_Q
                                when (_, _, '1001', _, '1') => __encoding aarch32_VMUL_i_T1A1_A // VMUL_i_A1_Q
                                when ('1', '00', '1100', _, '0') => __encoding aarch32_SHA256H_A1_A // SHA256H_A1
                                when (_, _, '1010', '0', '0') => __encoding aarch32_VPMAX_i_T1A1_A // VPMAX_i_A1
                                when ('1', '01', '0001', _, '1') => __encoding aarch32_VBIF_T1A1_A // VBSL_A1_Q
                                when (_, _, '1010', '0', '1') => __encoding aarch32_VPMAX_i_T1A1_A // VPMIN_i_A1
                                when (_, _, '1010', '1', _) => __UNALLOCATED
                                when ('1', '01', '1100', _, '0') => __encoding aarch32_SHA256H2_A1_A // SHA256H2_A1
                                when ('1', '1x', '1101', _, '0') => __encoding aarch32_VABD_f_A1_A // VABD_f_A1_Q
                                when ('1', '1x', '1110', _, '0') => __encoding aarch32_VCGT_r_T1A1_A
                                when ('1', '1x', '1110', _, '1') => __encoding aarch32_VACGE_A1_A // VACGT_A1_Q
                                when ('1', '1x', '1111', '0', '0') => __encoding aarch32_VPMAX_f_A1_A // VPMIN_f_A1
                                when ('1', '1x', '1111', _, '1') => __encoding aarch32_VMAXNM_A1_A // VMINNM_A1_Q
                                when ('1', _, '1000', _, '0') => __encoding aarch32_VSUB_i_T1A1_A // VSUB_i_A1_Q
                                when ('1', '10', '0001', _, '1') => __encoding aarch32_VBIF_T1A1_A // VBIT_A1_Q
                                when ('1', _, '1000', _, '1') => __encoding aarch32_VCEQ_r_T1A1_A
                                when ('1', _, '1001', _, '0') => __encoding aarch32_VMLA_i_T1A1_A // VMLS_i_A1_Q
                                when ('1', _, '1011', _, '0') => __encoding aarch32_VQRDMULH_T1A1_A // VQRDMULH_A1_Q
                                when ('1', '10', '1100', _, '0') => __encoding aarch32_SHA256SU1_A1_A // SHA256SU1_A1
                                when ('1', _, '1011', _, '1') => __encoding aarch32_VQRDMLAH_A1_A // VQRDMLAH_A1_Q
                                when ('1', '11', '0001', _, '1') => __encoding aarch32_VBIF_T1A1_A // VBIF_A1_Q
                                when ('1', _, '1100', _, '1') => __encoding aarch32_VQRDMLSH_A1_A // VQRDMLSH_A1_Q
                                when ('1', _, '1111', '1', '0') => __UNALLOCATED
                        when (_, _, '1', _, '0', _) =>
                            // a_simd_mulreg
                            case (25 +: 7, 24 +: 1, 23 +: 1, 22 +: 1, 20 +: 2, 12 +: 8, 10 +: 2, 7 +: 3, 6 +: 1, 5 +: 1, 4 +: 1, 0 +: 4) of
                                when (_, '0', _, _, '11', _, _, _, _, _, _, _) => // simd3reg_ext
                                    __field D 22 +: 1
                                    __field Vn 16 +: 4
                                    __field Vd 12 +: 4
                                    __field imm4 8 +: 4
                                    __field N 7 +: 1
                                    __field Q 6 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case () of
                                        when () => __encoding aarch32_VEXT_T1A1_A // VEXT_A1_Q
                                when (_, '1', _, _, '11', _, '0x', _, _, _, _, _) => // simd2reg_misc
                                    __field D 22 +: 1
                                    __field size 18 +: 2
                                    __field opc1 16 +: 2
                                    __field Vd 12 +: 4
                                    __field opc2 7 +: 4
                                    __field Q 6 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case (size, opc1, opc2, Q) of
                                        when (_, '00', '0000', _) => __encoding aarch32_VREV16_T1A1_A // VREV64_A1_Q
                                        when (_, '00', '0001', _) => __encoding aarch32_VREV16_T1A1_A // VREV32_A1_Q
                                        when (_, '00', '0010', _) => __encoding aarch32_VREV16_T1A1_A // VREV16_A1_Q
                                        when (_, '00', '0011', _) => __UNALLOCATED
                                        when (_, '00', '010x', _) => __encoding aarch32_VPADDL_T1A1_A // VPADDL_A1_Q
                                        when (_, '00', '0110', '0') => __encoding aarch32_AESE_A1_A // AESE_A1
                                        when (_, '00', '0110', '1') => __encoding aarch32_AESD_A1_A // AESD_A1
                                        when (_, '00', '0111', '0') => __encoding aarch32_AESMC_A1_A // AESMC_A1
                                        when (_, '00', '0111', '1') => __encoding aarch32_AESIMC_A1_A // AESIMC_A1
                                        when (_, '00', '1000', _) => __encoding aarch32_VCLS_T1A1_A // VCLS_A1_Q
                                        when ('00', '10', '0000', _) => __encoding aarch32_VSWP_T1A1_A // VSWP_A1_Q
                                        when (_, '00', '1001', _) => __encoding aarch32_VCLZ_T1A1_A // VCLZ_A1_Q
                                        when (_, '00', '1010', _) => __encoding aarch32_VCNT_T1A1_A // VCNT_A1_Q
                                        when (_, '00', '1011', _) => __encoding aarch32_VMVN_r_T1A1_A // VMVN_r_A1_Q
                                        when (_, '00', '110x', _) => __encoding aarch32_VPADAL_T1A1_A // VPADAL_A1_Q
                                        when (_, '00', '1110', _) => __encoding aarch32_VQABS_T1A1_A // VQABS_A1_Q
                                        when (_, '00', '1111', _) => __encoding aarch32_VQNEG_T1A1_A // VQNEG_A1_Q
                                        when (_, '01', 'x000', _) => __encoding aarch32_VCGT_i_A1_A // VCGT_i_A1_Q
                                        when (_, '01', 'x001', _) => __encoding aarch32_VCGE_i_A1_A // VCGE_i_A1_Q
                                        when (_, '01', 'x010', _) => __encoding aarch32_VCEQ_i_A1_A // VCEQ_i_A1_Q
                                        when (_, '01', 'x011', _) => __encoding aarch32_VCLE_i_A1_A // VCLE_i_A1_Q
                                        when (_, '01', 'x100', _) => __encoding aarch32_VCLT_i_A1_A // VCLT_i_A1_Q
                                        when (_, '01', 'x110', _) => __encoding aarch32_VABS_A1_A // VABS_A1_Q
                                        when (_, '01', 'x111', _) => __encoding aarch32_VNEG_A1_A // VNEG_A1_Q
                                        when (_, '01', '0101', '1') => __encoding aarch32_SHA1H_A1_A // SHA1H_A1
                                        when (_, '10', '0001', _) => __encoding aarch32_VTRN_T1A1_A // VTRN_A1_Q
                                        when (_, '10', '0010', _) => __encoding aarch32_VUZP_T1A1_A // VUZP_A1_Q
                                        when (_, '10', '0011', _) => __encoding aarch32_VZIP_T1A1_A // VZIP_A1_Q
                                        when (_, '10', '0100', '0') => __encoding aarch32_VMOVN_T1A1_A // VMOVN_A1
                                        when (_, '10', '0100', '1') => __encoding aarch32_VQMOVN_T1A1_A // VQMOVUN_A1
                                        when (_, '10', '0101', _) => __encoding aarch32_VQMOVN_T1A1_A // VQMOVN_A1
                                        when (_, '10', '0110', '0') => __encoding aarch32_VSHLL_T2A2_A // VSHLL_A2
                                        when (_, '10', '0111', '0') => __encoding aarch32_SHA1SU1_A1_A // SHA1SU1_A1
                                        when (_, '10', '0111', '1') => __encoding aarch32_SHA256SU0_A1_A // SHA256SU0_A1
                                        when (_, '10', '1000', _) => __encoding aarch32_VRINTA_asimd_A1_A // VRINTN_asimd_A1_Q
                                        when (_, '10', '1001', _) => __encoding aarch32_VRINTX_asimd_A1_A // VRINTX_asimd_A1_Q
                                        when (_, '10', '1010', _) => __encoding aarch32_VRINTA_asimd_A1_A // VRINTA_asimd_A1_Q
                                        when (_, '10', '1011', _) => __encoding aarch32_VRINTZ_asimd_A1_A // VRINTZ_asimd_A1_Q
                                        when (_, '10', '1100', '0') => __encoding aarch32_VCVT_hs_T1A1_A // VCVT_hs_A1
                                        when (_, '10', '1100', '1') => __UNALLOCATED
                                        when (_, '10', '1101', _) => __encoding aarch32_VRINTA_asimd_A1_A // VRINTM_asimd_A1_Q
                                        when (_, '10', '1110', '0') => __encoding aarch32_VCVT_hs_T1A1_A // VCVT_sh_A1
                                        when (_, '10', '1110', '1') => __UNALLOCATED
                                        when (_, '10', '1111', _) => __encoding aarch32_VRINTA_asimd_A1_A // VRINTP_asimd_A1_Q
                                        when (_, '11', '000x', _) => __encoding aarch32_VCVTA_asimd_A1_A // VCVTA_asimd_A1_Q
                                        when (_, '11', '001x', _) => __encoding aarch32_VCVTA_asimd_A1_A // VCVTN_asimd_A1_Q
                                        when (_, '11', '010x', _) => __encoding aarch32_VCVTA_asimd_A1_A // VCVTP_asimd_A1_Q
                                        when (_, '11', '011x', _) => __encoding aarch32_VCVTA_asimd_A1_A // VCVTM_asimd_A1_Q
                                        when (_, '11', '10x0', _) => __encoding aarch32_VRECPE_A1_A // VRECPE_A1_Q
                                        when (_, '11', '10x1', _) => __encoding aarch32_VRSQRTE_A1_A // VRSQRTE_A1_Q
                                        when (_, '11', '11xx', _) => __encoding aarch32_VCVT_is_A1_A // VCVT_is_A1_Q
                                when (_, '1', _, _, '11', _, '10', _, _, _, _, _) => // simd3reg_tbl
                                    __field D 22 +: 1
                                    __field Vn 16 +: 4
                                    __field Vd 12 +: 4
                                    __field len 8 +: 2
                                    __field N 7 +: 1
                                    __field op 6 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case () of
                                        when () => __encoding aarch32_VTBL_T1A1_A // VTBX_A1
                                when (_, '1', _, _, '11', _, '11', _, _, _, _, _) => // simd2reg_dup
                                    __field D 22 +: 1
                                    __field imm4 16 +: 4
                                    __field Vd 12 +: 4
                                    __field opc 7 +: 3
                                    __field Q 6 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case (opc) of
                                        when ('000') => __encoding aarch32_VDUP_s_T1A1_A // VDUP_s_A1_Q
                                        when ('001') => __UNALLOCATED
                                        when ('01x') => __UNALLOCATED
                                        when ('1xx') => __UNALLOCATED
                                when (_, _, _, _, !'11', _, _, _, '0', _, _, _) => // simd3reg_diff
                                    __field U 24 +: 1
                                    __field D 22 +: 1
                                    __field size 20 +: 2
                                    __field Vn 16 +: 4
                                    __field Vd 12 +: 4
                                    __field opc 8 +: 4
                                    __field N 7 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case (U, opc) of
                                        when (_, '0000') => __encoding aarch32_VADDL_T1A1_A // VADDL_A1
                                        when (_, '0001') => __encoding aarch32_VADDL_T1A1_A // VADDW_A1
                                        when (_, '0010') => __encoding aarch32_VSUBL_T1A1_A // VSUBL_A1
                                        when ('0', '0100') => __encoding aarch32_VADDHN_T1A1_A // VADDHN_A1
                                        when (_, '0011') => __encoding aarch32_VSUBL_T1A1_A // VSUBW_A1
                                        when ('0', '0110') => __encoding aarch32_VSUBHN_T1A1_A // VSUBHN_A1
                                        when ('0', '1001') => __encoding aarch32_VQDMLAL_T1A1_A // VQDMLAL_A1
                                        when (_, '0101') => __encoding aarch32_VABA_T2A2_A // VABAL_A1
                                        when ('0', '1011') => __encoding aarch32_VQDMLAL_T1A1_A // VQDMLSL_A1
                                        when ('0', '1101') => __encoding aarch32_VQDMULL_T1A1_A // VQDMULL_A1
                                        when (_, '0111') => __encoding aarch32_VABD_i_T2A2_A // VABDL_i_A1
                                        when (_, '1000') => __encoding aarch32_VMLA_i_T2A2_A // VMLAL_i_A1
                                        when (_, '1010') => __encoding aarch32_VMLA_i_T2A2_A // VMLSL_i_A1
                                        when ('1', '0100') => __encoding aarch32_VRADDHN_T1A1_A // VRADDHN_A1
                                        when ('1', '0110') => __encoding aarch32_VRSUBHN_T1A1_A // VRSUBHN_A1
                                        when (_, '11x0') => __encoding aarch32_VMUL_i_A2_A // VMULL_i_A1
                                        when ('1', '1001') => __UNALLOCATED
                                        when ('1', '1011') => __UNALLOCATED
                                        when ('1', '1101') => __UNALLOCATED
                                        when (_, '1111') => __UNALLOCATED
                                when (_, _, _, _, !'11', _, _, _, '1', _, _, _) => // simd2reg_scalar
                                    __field Q 24 +: 1
                                    __field D 22 +: 1
                                    __field size 20 +: 2
                                    __field Vn 16 +: 4
                                    __field Vd 12 +: 4
                                    __field opc 8 +: 4
                                    __field N 7 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case (Q, opc) of
                                        when (_, '000x') => __encoding aarch32_VMLA_s_A1_A // VMLA_s_A1_Q
                                        when ('0', '0011') => __encoding aarch32_VQDMLAL_T2A2_A // VQDMLAL_A2
                                        when (_, '0010') => __encoding aarch32_VMLA_s_T2A2_A // VMLAL_s_A1
                                        when ('0', '0111') => __encoding aarch32_VQDMLAL_T2A2_A // VQDMLSL_A2
                                        when (_, '010x') => __encoding aarch32_VMLA_s_A1_A // VMLS_s_A1_Q
                                        when ('0', '1011') => __encoding aarch32_VQDMULL_T2A2_A // VQDMULL_A2
                                        when (_, '0110') => __encoding aarch32_VMLA_s_T2A2_A // VMLSL_s_A1
                                        when (_, '100x') => __encoding aarch32_VMUL_s_A1_A // VMUL_s_A1_Q
                                        when ('1', '0011') => __UNALLOCATED
                                        when (_, '1010') => __encoding aarch32_VMUL_s_T2A2_A // VMULL_s_A1
                                        when ('1', '0111') => __UNALLOCATED
                                        when (_, '1100') => __encoding aarch32_VQDMULH_T2A2_A // VQDMULH_A2_Q
                                        when (_, '1101') => __encoding aarch32_VQRDMULH_T2A2_A // VQRDMULH_A2_Q
                                        when ('1', '1011') => __UNALLOCATED
                                        when (_, '1110') => __encoding aarch32_VQRDMLAH_A2_A // VQRDMLAH_A2_Q
                                        when (_, '1111') => __encoding aarch32_VQRDMLSH_A2_A // VQRDMLSH_A2_Q
                        when (_, _, '1', _, '1', _) =>
                            // a_simd_12reg
                            case (25 +: 7, 24 +: 1, 23 +: 1, 22 +: 1, 7 +: 15, 5 +: 2, 4 +: 1, 0 +: 4) of
                                when (_, _, _, _, '000xxxxxxxxxxx0', _, _, _) => // simd1reg_imm
                                    __field i 24 +: 1
                                    __field D 22 +: 1
                                    __field imm3 16 +: 3
                                    __field Vd 12 +: 4
                                    __field cmode 8 +: 4
                                    __field Q 6 +: 1
                                    __field op 5 +: 1
                                    __field imm4 0 +: 4
                                    case (cmode, op) of
                                        when ('0xx0', '0') => __encoding aarch32_VMOV_i_T1A1_A
                                        when ('0xx0', '1') => __encoding aarch32_VMVN_i_T1A1_A
                                        when ('0xx1', '0') => __encoding aarch32_VORR_i_T1A1_A
                                        when ('0xx1', '1') => __encoding aarch32_VBIC_i_T1A1_A
                                        when ('10x0', '0') => __encoding aarch32_VMOV_i_T1A1_A
                                        when ('10x0', '1') => __encoding aarch32_VMVN_i_T1A1_A
                                        when ('10x1', '0') => __encoding aarch32_VORR_i_T1A1_A
                                        when ('10x1', '1') => __encoding aarch32_VBIC_i_T1A1_A
                                        when ('11xx', '0') => __encoding aarch32_VMOV_i_T1A1_A
                                        when ('110x', '1') => __encoding aarch32_VMVN_i_T1A1_A
                                        when ('1110', '1') => __encoding aarch32_VMOV_i_T1A1_A
                                        when ('1111', '1') => __UNALLOCATED
                                when (_, _, _, _, !'000xxxxxxxxxxx0', _, _, _) => // simd2reg_shift
                                    __field U 24 +: 1
                                    __field D 22 +: 1
                                    __field imm3H 19 +: 3
                                    __field imm3L 16 +: 3
                                    __field Vd 12 +: 4
                                    __field opc 8 +: 4
                                    __field L 7 +: 1
                                    __field Q 6 +: 1
                                    __field M 5 +: 1
                                    __field Vm 0 +: 4
                                    case (U, imm3H:L, imm3L, opc, Q) of
                                        when (_, !'0000', _, '0000', _) => __encoding aarch32_VSHR_T1A1_A // VSHR_A1_Q
                                        when (_, !'0000', _, '0001', _) => __encoding aarch32_VSRA_T1A1_A // VSRA_A1_Q
                                        when (_, !'0000', '000', '1010', '0') => __encoding aarch32_VMOVL_T1A1_A // VMOVL_A1
                                        when (_, !'0000', _, '0010', _) => __encoding aarch32_VRSHR_T1A1_A // VRSHR_A1_Q
                                        when (_, !'0000', _, '0011', _) => __encoding aarch32_VRSRA_T1A1_A // VRSRA_A1_D
                                        when (_, !'0000', _, '0111', _) => __encoding aarch32_VQSHL_i_T1A1_A // VQSHL_i_A1_Q
                                        when (_, !'0000', _, '1001', '0') => __encoding aarch32_VQSHRN_T1A1_A // VQSHRN_A1
                                        when (_, !'0000', _, '1001', '1') => __encoding aarch32_VQRSHRN_T1A1_A // VQRSHRN_A1
                                        when (_, !'0000', _, '1010', '0') => __encoding aarch32_VSHLL_T1A1_A // VSHLL_A1
                                        when (_, !'0000', _, '11xx', _) => __encoding aarch32_VCVT_xs_A1_A // VCVT_xs_A1_Q
                                        when ('0', !'0000', _, '0101', _) => __encoding aarch32_VSHL_i_T1A1_A // VSHL_i_A1_Q
                                        when ('0', !'0000', _, '1000', '0') => __encoding aarch32_VSHRN_T1A1_A // VSHRN_A1
                                        when ('0', !'0000', _, '1000', '1') => __encoding aarch32_VRSHRN_T1A1_A // VRSHRN_A1
                                        when ('1', !'0000', _, '0100', _) => __encoding aarch32_VSRI_T1A1_A // VSRI_A1_Q
                                        when ('1', !'0000', _, '0101', _) => __encoding aarch32_VSLI_T1A1_A // VSLI_A1_Q
                                        when ('1', !'0000', _, '0110', _) => __encoding aarch32_VQSHL_i_T1A1_A // VQSHLU_i_A1_Q
                                        when ('1', !'0000', _, '1000', '0') => __encoding aarch32_VQSHRN_T1A1_A // VQSHRUN_A1
                                        when ('1', !'0000', _, '1000', '1') => __encoding aarch32_VQRSHRN_T1A1_A // VQRSHRUN_A1
                when (_, '1x', _, '1', _) =>
                    // uncondhints
                    case (26 +: 6, 21 +: 5, 20 +: 1, 5 +: 15, 4 +: 1, 0 +: 4) of
                        when (_, '00xx1', _, _, _, _) => __UNALLOCATED
                        when (_, '01001', _, _, _, _) => __UNALLOCATED
                        when (_, '01011', _, _, _, _) => // barriers
                            __field opcode 4 +: 4
                            __field option 0 +: 4
                            case (opcode, option) of
                                when ('0000', _) => __UNPREDICTABLE
                                when ('0001', _) => __encoding aarch32_CLREX_A1_A // CLREX_A1
                                when ('001x', _) => __UNPREDICTABLE
                                when ('0100', _) => __encoding aarch32_DSB_A1_A // DSB_A1
                                when ('0100', '0000') => __encoding aarch32_SSBB_A1_A // SSBB_A1
                                when ('0100', '0100') => __encoding aarch32_PSSBB_A1_A // PSSBB_A1
                                when ('0101', _) => __encoding aarch32_DMB_A1_A // DMB_A1
                                when ('0110', _) => __encoding aarch32_ISB_A1_A // ISB_A1
                                when ('0111', _) => __encoding aarch32_SB_A1_A // SB_A1
                                when ('1xxx', _) => __UNPREDICTABLE
                        when (_, '011x1', _, _, _, _) => __UNALLOCATED
                        when (_, '0xxx0', _, _, _, _) => // preload_imm
                            __field D 24 +: 1
                            __field U 23 +: 1
                            __field R 22 +: 1
                            __field Rn 16 +: 4
                            __field imm12 0 +: 12
                            case (D, R, Rn) of
                                when ('0', '0', _) => __NOP
                                when ('0', '1', _) => __encoding aarch32_PLI_i_A1_A // PLI_i_A1
                                when ('1', _, '1111') => __encoding aarch32_PLD_l_A1_A // PLD_l_A1
                                when ('1', '0', !'1111') => __encoding aarch32_PLD_i_A1_A // PLDW_i_A1
                                when ('1', '1', !'1111') => __encoding aarch32_PLD_i_A1_A // PLD_i_A1
                        when (_, '1xxx0', _, _, '0', _) => // preload_reg
                            __field D 24 +: 1
                            __field U 23 +: 1
                            __field o2 22 +: 1
                            __field Rn 16 +: 4
                            __field imm5 7 +: 5
                            __field type1 5 +: 2
                            __field Rm 0 +: 4
                            case (D, o2) of
                                when ('0', '0') => __NOP
                                when ('0', '1') => __encoding aarch32_PLI_r_A1_A // PLI_r_A1_RRX
                                when ('1', '0') => __encoding aarch32_PLD_r_A1_A // PLDW_r_A1_RRX
                                when ('1', '1') => __encoding aarch32_PLD_r_A1_A // PLD_r_A1_RRX
                        when (_, '1xxx1', _, _, '0', _) => __UNALLOCATED
                        when (_, '1xxxx', _, _, '1', _) => __UNPREDICTABLE
                when (_, '10', _, '0', _) =>
                    // advsimdls
                    case (24 +: 8, 23 +: 1, 21 +: 2, 20 +: 1, 12 +: 8, 10 +: 2, 0 +: 10) of
                        when (_, '0', _, _, _, _, _) => // ldstv_ms
                            __field D 22 +: 1
                            __field L 21 +: 1
                            __field Rn 16 +: 4
                            __field Vd 12 +: 4
                            __field type1 8 +: 4
                            __field size 6 +: 2
                            __field align 4 +: 2
                            __field Rm 0 +: 4
                            case (L, type1) of
                                when ('0', '000x') => __encoding aarch32_VST4_m_T1A1_A // VST4_m_A1_posti
                                when ('0', '0010') => __encoding aarch32_VST1_m_T1A1_A
                                when ('0', '0011') => __encoding aarch32_VST2_m_T1A1_A
                                when ('0', '010x') => __encoding aarch32_VST3_m_T1A1_A // VST3_m_A1_posti
                                when ('0', '0110') => __encoding aarch32_VST1_m_T1A1_A
                                when ('0', '0111') => __encoding aarch32_VST1_m_T1A1_A
                                when ('0', '100x') => __encoding aarch32_VST2_m_T1A1_A
                                when ('0', '1010') => __encoding aarch32_VST1_m_T1A1_A
                                when ('1', '000x') => __encoding aarch32_VLD4_m_T1A1_A // VLD4_m_A1_posti
                                when ('1', '0010') => __encoding aarch32_VLD1_m_T1A1_A
                                when ('1', '0011') => __encoding aarch32_VLD2_m_T1A1_A
                                when ('1', '010x') => __encoding aarch32_VLD3_m_T1A1_A // VLD3_m_A1_posti
                                when (_, '1011') => __UNALLOCATED
                                when ('1', '0110') => __encoding aarch32_VLD1_m_T1A1_A
                                when ('1', '0111') => __encoding aarch32_VLD1_m_T1A1_A
                                when (_, '11xx') => __UNALLOCATED
                                when ('1', '100x') => __encoding aarch32_VLD2_m_T1A1_A
                                when ('1', '1010') => __encoding aarch32_VLD1_m_T1A1_A
                        when (_, '1', _, _, _, '11', _) => // ldv_ssall
                            __field D 22 +: 1
                            __field L 21 +: 1
                            __field Rn 16 +: 4
                            __field Vd 12 +: 4
                            __field N 8 +: 2
                            __field size 6 +: 2
                            __field T 5 +: 1
                            __field a 4 +: 1
                            __field Rm 0 +: 4
                            case (L, N, a) of
                                when ('0', _, _) => __UNALLOCATED
                                when ('1', '00', _) => __encoding aarch32_VLD1_a_T1A1_A // VLD1_a_A1_posti
                                when ('1', '01', _) => __encoding aarch32_VLD2_a_T1A1_A // VLD2_a_A1_posti
                                when ('1', '10', '0') => __encoding aarch32_VLD3_a_T1A1_A // VLD3_a_A1_posti
                                when ('1', '10', '1') => __UNALLOCATED
                                when ('1', '11', _) => __encoding aarch32_VLD4_a_T1A1_A // VLD4_a_A1_posti
                        when (_, '1', _, _, _, !'11', _) => // ldstv_ssone
                            __field D 22 +: 1
                            __field L 21 +: 1
                            __field Rn 16 +: 4
                            __field Vd 12 +: 4
                            __field size 10 +: 2
                            __field N 8 +: 2
                            __field index_align 4 +: 4
                            __field Rm 0 +: 4
                            case (L, size, N) of
                                when ('0', '00', '00') => __encoding aarch32_VST1_1_T1A1_A
                                when ('0', '00', '01') => __encoding aarch32_VST2_1_T1A1_A
                                when ('0', '00', '10') => __encoding aarch32_VST3_1_T1A1_A
                                when ('0', '00', '11') => __encoding aarch32_VST4_1_T1A1_A
                                when ('0', '01', '00') => __encoding aarch32_VST1_1_T1A1_A
                                when ('0', '01', '01') => __encoding aarch32_VST2_1_T1A1_A
                                when ('0', '01', '10') => __encoding aarch32_VST3_1_T1A1_A
                                when ('0', '01', '11') => __encoding aarch32_VST4_1_T1A1_A
                                when ('0', '10', '00') => __encoding aarch32_VST1_1_T1A1_A
                                when ('0', '10', '01') => __encoding aarch32_VST2_1_T1A1_A
                                when ('0', '10', '10') => __encoding aarch32_VST3_1_T1A1_A
                                when ('0', '10', '11') => __encoding aarch32_VST4_1_T1A1_A
                                when ('1', '00', '00') => __encoding aarch32_VLD1_1_T1A1_A
                                when ('1', '00', '01') => __encoding aarch32_VLD2_1_T1A1_A
                                when ('1', '00', '10') => __encoding aarch32_VLD3_1_T1A1_A
                                when ('1', '00', '11') => __encoding aarch32_VLD4_1_T1A1_A
                                when ('1', '01', '00') => __encoding aarch32_VLD1_1_T1A1_A
                                when ('1', '01', '01') => __encoding aarch32_VLD2_1_T1A1_A
                                when ('1', '01', '10') => __encoding aarch32_VLD3_1_T1A1_A
                                when ('1', '01', '11') => __encoding aarch32_VLD4_1_T1A1_A
                                when ('1', '10', '00') => __encoding aarch32_VLD1_1_T1A1_A
                                when ('1', '10', '01') => __encoding aarch32_VLD2_1_T1A1_A
                                when ('1', '10', '10') => __encoding aarch32_VLD3_1_T1A1_A
                                when ('1', '10', '11') => __encoding aarch32_VLD4_1_T1A1_A
                when (_, '11', _, '0', _) => __UNPREDICTABLE
