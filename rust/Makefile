# SPDX-License-Identifier: GPL-2.0
#
# Minimal rust/ Makefile, simplified from the 6.12 reference.
# Dropped: doctest, rustdoc, rust-analyzer, test library.
# Kept: core, compiler_builtins, alloc, bindings, kernel,
# uapi, helpers, exports, build_error, libmacros.so.
#
# Differences from the reference:
# - 4.19 does not generate include/generated/rustc_cfg through
#   Kconfig, so that file is created here from .config (only
#   =y/=m symbols, one --cfg per line).
# - Exports use EXPORT_SYMBOL_GPL through the
#   EXPORT_SYMBOL_RUST_GPL macro in exports.c (already in the
#   reference), not the new 6.12 macro.
# - No objtool for Rust objects (this tree's objtool does not
#   process Rust objects).

obj-$(CONFIG_RUST) += core.o compiler_builtins.o
always-$(CONFIG_RUST) += exports_core_generated.h

obj-$(CONFIG_RUST) += helpers/helpers.o
CFLAGS_REMOVE_helpers/helpers.o = -Wmissing-prototypes -Wmissing-declarations

always-$(CONFIG_RUST) += libmacros.so

always-$(CONFIG_RUST) += bindings/bindings_generated.rs bindings/bindings_helpers_generated.rs
obj-$(CONFIG_RUST) += alloc.o bindings.o kernel.o
always-$(CONFIG_RUST) += exports_alloc_generated.h exports_helpers_generated.h \
    exports_bindings_generated.h exports_kernel_generated.h

always-$(CONFIG_RUST) += uapi/uapi_generated.rs
obj-$(CONFIG_RUST) += uapi.o

always-$(CONFIG_RUST) += build_error.o

obj-$(CONFIG_RUST) += exports.o

ifdef CONFIG_RUST

rustc_sysroot := $(shell MAKEFLAGS= $(RUSTC) $(rust_flags) --print sysroot)
RUST_LIB_SRC ?= $(rustc_sysroot)/lib/rustlib/src/rust/library

core-cfgs = \
    --cfg no_fp_fmt_parse

alloc-cfgs = \
    --cfg no_global_oom_handling \
    --cfg no_rc \
    --cfg no_sync

# rustc_cfg: 4.19 Kconfig does not generate it, so build it from .config.
# One --cfg per symbol that is =y or =m.
quiet_cmd_rustc_cfg = RUSTCFG $@
      cmd_rustc_cfg = \
	mkdir -p $(dir $@); \
	grep -E '^CONFIG_[A-Z0-9_]+=y$$|^CONFIG_[A-Z0-9_]+=m$$' $(KCONFIG_CONFIG) | \
	sed -E 's/^CONFIG_([A-Z0-9_]+)=[ym]$$/--cfg=CONFIG_\1/' > $@.tmp; \
	mv $@.tmp $@

$(objtree)/include/generated/rustc_cfg: $(KCONFIG_CONFIG) FORCE
	$(call if_changed,rustc_cfg)

ifdef CONFIG_CC_IS_CLANG
bindgen_c_flags = $(c_flags)
else
bindgen_c_flags = $(c_flags)
endif

bindgen_c_flags_final = $(bindgen_c_flags) -D__BINDGEN__

quiet_cmd_bindgen = BINDGEN $@
      cmd_bindgen = \
	mkdir -p $(dir $@); $(BINDGEN) $< $(bindgen_target_flags) \
		--use-core --with-derive-default --ctypes-prefix core::ffi --no-layout-tests \
		--no-debug '.*' --enable-function-attribute-detection \
		-o $@ -- $(bindgen_c_flags_final) -DMODULE \
		$(bindgen_target_cflags) $(bindgen_target_extra)

$(obj)/bindings/bindings_generated.rs: private bindgen_target_flags = \
    $(shell grep -Ev '^#|^$$' $(srctree)/$(src)/bindgen_parameters)
$(obj)/bindings/bindings_generated.rs: private bindgen_target_extra = ; \
    sed -Ei 's/pub const RUST_CONST_HELPER_([a-zA-Z0-9_]*)/pub const \1/g' $@
$(obj)/bindings/bindings_generated.rs: $(src)/bindings/bindings_helper.h \
    $(srctree)/$(src)/bindgen_parameters FORCE
	$(call if_changed_dep,bindgen)

$(obj)/uapi/uapi_generated.rs: private bindgen_target_flags = \
    $(shell grep -Ev '^#|^$$' $(srctree)/$(src)/bindgen_parameters)
$(obj)/uapi/uapi_generated.rs: $(src)/uapi/uapi_helper.h \
    $(srctree)/$(src)/bindgen_parameters FORCE
	$(call if_changed_dep,bindgen)

$(obj)/bindings/bindings_helpers_generated.rs: private bindgen_target_flags = \
    --blocklist-type '.*' --allowlist-var '' \
    --allowlist-function 'rust_helper_.*'
$(obj)/bindings/bindings_helpers_generated.rs: private bindgen_target_cflags = \
    -I$(objtree)/$(obj) -Wno-missing-prototypes -Wno-missing-declarations
$(obj)/bindings/bindings_helpers_generated.rs: private bindgen_target_extra = ; \
    sed -Ei 's/pub fn rust_helper_([a-zA-Z0-9_]*)/#[link_name="rust_helper_\1"]\n    pub fn \1/g' $@
$(obj)/bindings/bindings_helpers_generated.rs: $(src)/helpers/helpers.c FORCE
	$(call if_changed_dep,bindgen)

quiet_cmd_exports = EXPORTS $@
      cmd_exports = \
	$(NM) -p --defined-only $< \
		| awk '$$2~/(T|R|D|B)/ && $$3!~/__cfi/ {printf "EXPORT_SYMBOL_RUST_GPL(%s);\n",$$3}' > $@

$(obj)/exports_core_generated.h: $(obj)/core.o FORCE
	$(call if_changed,exports)

$(obj)/exports_alloc_generated.h: $(obj)/alloc.o FORCE
	$(call if_changed,exports)

$(obj)/exports_helpers_generated.h: $(obj)/helpers/helpers.o FORCE
	$(call if_changed,exports)

$(obj)/exports_bindings_generated.h: $(obj)/bindings.o FORCE
	$(call if_changed,exports)

$(obj)/exports_kernel_generated.h: $(obj)/kernel.o FORCE
	$(call if_changed,exports)

# exports.o must wait for the generated headers, because
# always-targets are built after builtin-targets in this
# tree's Makefile.build.
$(obj)/exports.o: $(obj)/exports_core_generated.h $(obj)/exports_alloc_generated.h \
    $(obj)/exports_helpers_generated.h $(obj)/exports_bindings_generated.h \
    $(obj)/exports_kernel_generated.h

quiet_cmd_rustc_procmacro = RUSTC P $@
      cmd_rustc_procmacro = \
	$(HOSTRUSTC) $(KBUILD_HOSTRUSTFLAGS) \
		-Clinker-flavor=gcc -Clinker=$(HOSTCC) \
		-Clink-args='$(call escsq,$(KBUILD_HOSTLDFLAGS))' \
		--emit=dep-info=$(depfile) --emit=link=$@ --extern proc_macro \
		--crate-type proc-macro \
		--crate-name $(patsubst lib%.so,%,$(notdir $@)) $<

$(obj)/libmacros.so: $(src)/macros/lib.rs FORCE
	+$(call if_changed_dep,rustc_procmacro)

quiet_cmd_rustc_library = RUSTC L $@
      cmd_rustc_library = \
	OBJTREE=$(abspath $(objtree)) \
	$(RUSTC) \
		$(filter-out $(skip_flags),$(rust_flags) $(rustc_target_flags)) \
		@$(objtree)/include/generated/rustc_cfg \
		--emit=dep-info=$(depfile) --emit=obj=$@ \
		--emit=metadata=$(dir $@)$(patsubst %.o,lib%.rmeta,$(notdir $@)) \
		--crate-type rlib -L$(objtree)/$(obj) \
		--crate-name $(patsubst %.o,%,$(notdir $@)) $< \
		--sysroot=/dev/null \
	$(if $(rustc_objcopy),;$(OBJCOPY) $(rustc_objcopy) $@) \
	; sed -i '/env-dep/d' $(depfile)

define rule_rustc_library
	$(call cmd_and_fixdep,rustc_library)
endef

redirect-intrinsics = \
	__addsf3 __eqsf2 __extendsfdf2 __gesf2 __lesf2 __ltsf2 __mulsf3 __nesf2 __truncdfsf2 __unordsf2 \
	__adddf3 __divdf3 __eqdf2 __fixdfdi __floatdidf __gedf2 __gtdf2 __ledf2 __ltdf2 __muldf3 __unorddf2 \
	__muloti4 __multi3 \
	__udivmodti4 __udivti3 __umodti3 \
	__ashrti3 __ashlti3 __lshrti3

$(obj)/core.o: private skip_clippy = 1
$(obj)/core.o: private skip_flags = -Wunreachable_pub
$(obj)/core.o: private rustc_objcopy = $(foreach sym,$(redirect-intrinsics),--redefine-sym $(sym)=__rust$(sym))
$(obj)/core.o: private rustc_target_flags = $(core-cfgs)
$(obj)/core.o: $(RUST_LIB_SRC)/core/src/lib.rs \
    $(objtree)/include/generated/rustc_cfg FORCE
	+$(call if_changed_rule,rustc_library)

$(obj)/compiler_builtins.o: private rustc_objcopy = -w -W '__*'
$(obj)/compiler_builtins.o: $(src)/compiler_builtins.rs $(obj)/core.o FORCE
	+$(call if_changed_rule,rustc_library)

$(obj)/alloc.o: private skip_clippy = 1
$(obj)/alloc.o: private skip_flags = -Wunreachable_pub
$(obj)/alloc.o: private rustc_target_flags = $(alloc-cfgs)
$(obj)/alloc.o: $(RUST_LIB_SRC)/alloc/src/lib.rs $(obj)/compiler_builtins.o FORCE
	+$(call if_changed_rule,rustc_library)

$(obj)/build_error.o: $(src)/build_error.rs $(obj)/compiler_builtins.o FORCE
	+$(call if_changed_rule,rustc_library)

$(obj)/bindings.o: $(src)/bindings/lib.rs \
    $(obj)/compiler_builtins.o \
    $(obj)/bindings/bindings_generated.rs \
    $(obj)/bindings/bindings_helpers_generated.rs FORCE
	+$(call if_changed_rule,rustc_library)

$(obj)/uapi.o: $(src)/uapi/lib.rs \
    $(obj)/compiler_builtins.o \
    $(obj)/uapi/uapi_generated.rs FORCE
	+$(call if_changed_rule,rustc_library)

$(obj)/kernel.o: private rustc_target_flags = --extern alloc \
    --extern build_error --extern macros --extern bindings --extern uapi
$(obj)/kernel.o: $(src)/kernel/lib.rs $(obj)/alloc.o $(obj)/build_error.o \
    $(obj)/libmacros.so $(obj)/bindings.o $(obj)/uapi.o FORCE
	+$(call if_changed_rule,rustc_library)

endif # CONFIG_RUST
