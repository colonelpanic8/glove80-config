{ stdenvNoCC, lib, buildPackages
, cmake, ninja, dtc, gcc-arm-embedded, protobuf
, zephyr
, board ? "glove80_lh"
, shield ? null
, keymap ? null
, kconfig ? null
, extraModules ? []
, snippets ? []
, studioMessagesOverlay ? null
}:


let
  # from zephyr/scripts/requirements-base.txt
  packageOverrides = pyself: pysuper: {
    can = pysuper.can.overrideAttrs (_: {
      # horribly flaky test suite full of assertions about timing.
      # >       assert 0.1 <= took < inc(0.3)
      # E       assert 0.31151700019836426 < 0.3
      # E        +  where 0.3 = inc(0.3)
      doCheck = false;
      doInstallCheck = false;
    });

    canopen = pysuper.can.overrideAttrs (_: {
      # Also has timing sensitive tests
      #         task = self.network.send_periodic(0x123, [1, 2, 3], 0.01)
      #         time.sleep(0.1)
      # >       self.assertTrue(9 <= bus.queue.qsize() <= 11)
      # E       AssertionError: False is not true
      doCheck = false;
      doInstallCheck = false;
    });
  };

  python = (buildPackages.python3.override { inherit packageOverrides; }).withPackages (ps: with ps; [
    pyelftools
    pyyaml
    canopen
    packaging
    progress
    anytree
    ps.protobuf
    setuptools
    intelhex

    # TODO: this was required but not in shell.nix
    pykwalify
  ]);

  requiredZephyrModules = [
    "cmsis" "hal_nordic" "tinycrypt" "lvgl" "picolibc" "segger" "cirque-input-module"
    "nanopb" "zmk-studio-messages"
  ];

  # Some Zephyr modules seemingly need a symlink indirection (modulePath),
  # others don't (src).
  # This is not the best way to fix it, but it works around the problem.
  directZephyrModules = [ "cirque-input-module" ];

  patchedNanopbModule = buildPackages.runCommand "zmk-module-nanopb-patched" {
    nativeBuildInputs = [ python ];
  } ''
    cp -R ${zephyr.modules.nanopb.src} $out
    chmod -R u+w $out
    patchShebangs --build $out
  '';

  studioMessagesModule = zephyr.modules.zmk-studio-messages;
  patchedStudioMessagesModule =
    if studioMessagesOverlay == null then studioMessagesModule.modulePath
    else buildPackages.runCommand "zmk-studio-messages-patched" {} ''
      cp -R ${studioMessagesModule.src} $out
      chmod -R u+w $out
      cp -R ${studioMessagesOverlay}/. $out/
    '';

  zephyrModuleDeps =
    map (name:
      let module = zephyr.modules.${name};
      in if name == "nanopb" then patchedNanopbModule
         else if name == "zmk-studio-messages" then patchedStudioMessagesModule
         else if builtins.elem module.src.name directZephyrModules then module.src
         else module.modulePath
    ) requiredZephyrModules;
in

stdenvNoCC.mkDerivation {
  name = "zmk_${board}";

  sourceRoot = "source/app";

  src = builtins.path {
    name = "source";
    path = ./..;
    filter = path: type:
      let relPath = lib.removePrefix (toString ./.. + "/") (toString path);
      in (lib.cleanSourceFilter path type) && ! (
        # Meta files
        relPath == "nix" || lib.hasSuffix ".nix" path ||
        # Transient state
        relPath == "build" || relPath == ".west" ||
        # Fetched by west
        relPath == "modules" || relPath == "tools" || relPath == "zephyr" ||
        # Not part of ZMK
        relPath == "lambda" || relPath == ".github"
      );
    };

  preConfigure = ''
    cmakeFlagsArray+=("-DUSER_CACHE_DIR=$TEMPDIR/.cache")
  '';

  cmakeFlags = [
    # "-DZephyrBuildConfiguration_ROOT=${zephyr}/zephyr"
    # TODO: is this required? if not, why not?
    "-DZEPHYR_BASE=${zephyr}/zephyr"
    "-DBOARD_ROOT=."
    "-DBOARD=${board}"
    "-DZEPHYR_TOOLCHAIN_VARIANT=gnuarmemb"
    "-DGNUARMEMB_TOOLCHAIN_PATH=${gcc-arm-embedded}"
    # TODO: maybe just use a cross environment for this gcc
    "-DCMAKE_C_COMPILER=${gcc-arm-embedded}/bin/arm-none-eabi-gcc"
    "-DCMAKE_CXX_COMPILER=${gcc-arm-embedded}/bin/arm-none-eabi-g++"
    "-DCMAKE_AR=${gcc-arm-embedded}/bin/arm-none-eabi-ar"
    "-DCMAKE_RANLIB=${gcc-arm-embedded}/bin/arm-none-eabi-ranlib"
    "-DZEPHYR_MODULES=${lib.concatStringsSep ";" zephyrModuleDeps}"
  ] ++
  (lib.optional (shield != null) "-DSHIELD=${shield}") ++
  (lib.optional (keymap != null) "-DKEYMAP_FILE=${keymap}") ++
  (lib.optional (kconfig != null) "-DEXTRA_CONF_FILE=${kconfig}") ++
  (lib.optional (extraModules != []) "-DZMK_EXTRA_MODULES=${lib.concatStringsSep ";" extraModules}") ++
  (lib.optional (snippets != []) "-DSNIPPET=${lib.concatStringsSep ";" snippets}");

  nativeBuildInputs = [ cmake ninja python protobuf dtc gcc-arm-embedded ];
  buildInputs = [ zephyr ];

  installPhase = ''
    mkdir $out
    cp zephyr/zmk.{uf2,hex,bin,elf} $out
    cp zephyr/.config $out/zmk.kconfig
    cp zephyr/zephyr.dts $out/zmk.dts
  '';

  passthru = { inherit zephyrModuleDeps; };
}
