# Inventário da cópia durável dos relatórios do caminho entre pares

**Data da cópia:** 2026-09-09
**Origem:** `/private/tmp/claude-501/-Users-dev-alexandre-SEELE/00073a07-0caf-483e-90e3-5cb49b37bf06/scratchpad/wt2/.superpowers/sdd/2026-09-06-caminho-entre-pares`
**Destino:** `docs/superpowers/sdd/2026-09-06-caminho-entre-pares` (nesta branch)

## Por que esta cópia existe

Os relatórios das onze tarefas do caminho entre pares, a revisão final e os
pacotes de diff moravam **só** num worktree sob `/private/tmp`. Aquele diretório
é apagado no reboot, e o conteúdo dele não estava em nenhum commit: o
`.superpowers/sdd/.gitignore` do próprio worktree o mantinha fora do git de
propósito. Um reboot levava 2,4 MB de raciocínio registrado — as medições, as
provas por reversão, os motivos de cada conserto — sem aviso nenhum.

A cópia foi feita **antes** de qualquer merge, que é a ordem que importa: se a
integração desse errado, os relatórios já estariam salvos.

## A origem não foi tocada

Só leitura. Conferido depois da cópia, na origem:

```
$ git -C …/scratchpad/wt2 status --short
 M crates/seele-conformance/tests/tela_por_um_par.rs
 M crates/seele-core/src/enlace.rs
 M crates/seele-server/src/session.rs
```

As mesmas três linhas de antes da cópia, e nenhum arquivo novo. Ver a seção
«o que não foi commitado» no relatório ao lado: essas três modificações são
trabalho não commitado de outra sessão, e também foram preservadas — como
patch, sem entrar na integração.

## Como conferir que a cópia continua íntegra

O sha256 e o tamanho de cada arquivo estão na tabela abaixo, e a conferência
não depende mais da origem:

```sh
python3 docs/superpowers/sdd/2026-09-09-integracao-malha-e-mods/conferir-inventario.py
```

Na hora da cópia, o sha256 de cada arquivo **no destino** foi comparado com o
da origem, um a um: os 50 bateram.

## Os 50 arquivos

| sha256 | bytes | arquivo |
|---|---:|---|
| `2d88d952066b1cdcbd9e94e75681e5ead07673b86bd4b144224654c9883be97b` | 4714 | `fecho-brief.md` |
| `7f5268c0b601b59a765cf8f885aa5975d8c1e80ab316c666cea3a493f0ccdeb5` | 12409 | `onda-final-brief.md` |
| `9a88b1a243218263a6557156ffc33c981c3eb13f6b585eece8c0037995b237ef` | 123923 | `onda-final-package.md` |
| `e1e9c110d169a1e174a929dca37759191f78c40210a86f45fc4220009cbad0f5` | 24225 | `onda-final-report.md` |
| `37f70e71fd7dce5e95c9313c26dbfa29d7e408ca7c4c45b3b21cba95ecfe266f` | 50240 | `progress.md` |
| `3589d2ba67334364cc31ee7220385f84325684e58c501ebc0e8ef3f28d200597` | 109659 | `review-120485e..4f63842.diff` |
| `0888e75cbbd6e32aed9a0fe1971586633f5474abaae918e7d234569116acb80f` | 366119 | `review-15a0406..725be01.diff` |
| `35f67a857c7275e5721a5bc17ea3127219c6145f73717c43e25fc85d7ba9e10c` | 55615 | `review-32d6ace..51f6a0f.diff` |
| `f8dba11294aefbcec859fc75ed04350f67377c83adfe21afb45bf716641e3c17` | 8822 | `review-3938861..861b3b4.diff` |
| `541fe3e0c463da9d5c96cd7d2353eab3719a3cbc3a6d2cb315152e2d7fb571e0` | 7969 | `review-3aa13aa..c78b671.diff` |
| `1f6dd8a2d80b54a38d9378d248617509b66a117e027aa0092f98b669a89b8898` | 6160 | `review-4587999..b3c88c9.diff` |
| `e428f97b60a8965d8fa75c75fe494c0891739bfbe07e51701deb412ad0a09988` | 31044 | `review-48067d4..5c5fd8b.diff` |
| `6907c75d0433249c09f4b9702b8230c74e245260ad92d0050b0037b3ba930659` | 37723 | `review-499c9ec..8be5ac3.diff` |
| `e93861c4b172620905da3b682377210676fd8775f94e185003a2ddc5845d97b3` | 28445 | `review-4f63842..4587999.diff` |
| `051556be5ead114d444b01d0ae5504b9f853ad4f767f42d0505d5ce36a63ea44` | 53581 | `review-51f6a0f..86aea6d.diff` |
| `7c4982cd57407e05d388f9f2e5c32f523887e5f48f07e1b1933562969a280b4e` | 25548 | `review-5c5fd8b..c562e81.diff` |
| `8f02fabdf4162ced4172f24222b90d4564522707860cd9a90453402dd90c73df` | 4844 | `review-68ea9e5..dfd6584.diff` |
| `e650710d1fbb06dc33e339042c5f5f267417beec612e228bd157ed48aaa903f7` | 37596 | `review-81787f4..32d6ace.diff` |
| `f89cbb44fee50339a61b8858d9b8083b5d65c69abf0d311e2f77af76d994266a` | 18767 | `review-861b3b4..a6eee0d.diff` |
| `59e1a228d5805ebac4458c39039fc34d5a0bae5211241b7e5fc996905a5a2d0e` | 15780 | `review-86aea6d..499c9ec.diff` |
| `b79cc9eb34493ed02e9fda28392a05c3e366709b744383548d7988847c6c4020` | 9168 | `review-8be5ac3..120485e.diff` |
| `b75e9d8719f7ffb0343e93f2fdb936be4bd1d8b7b9792a6e46a836c2f4d1c053` | 1580 | `review-a3deb54..3938861.diff` |
| `74251605bb48f550f440338f08299f62513541bd8bb5557b2e9c88724d5fc645` | 36460 | `review-a4e0f66..48067d4.diff` |
| `d6aa04f34877f24759b7851ce80cff33623efb4ed6841eaba65ba3c70c2d93d9` | 19440 | `review-a6eee0d..5497c97.diff` |
| `e7ce0d83c2ce0e5c262e12bfc9a010b9d46a7eb9204b465e9c7f9b0eff7667c7` | 24545 | `review-c562e81..1edf3b0.diff` |
| `e3d3fd6f3c90d04236ea45f2d0f6cf6b01f0fdae18c8b9e5bb6a94fbd697f39f` | 3862 | `review-c78b671..a3deb54.diff` |
| `f2ceabe889f060502bd7649fcf8b541fa69c2b88e81c893a6aa0ffd75b64cd73` | 18040 | `review-dfd6584..4888f37.diff` |
| `ae7ca1d88e3283e52fe7411189ad2ec188e43aa9ca6ae5eeabd81e0154745eab` | 1063344 | `revisao-final.md` |
| `2dd4b74b922270b13f149494920e06721c47edfa232ea3ada0d556743a4f5e0a` | 7400 | `task-1-brief.md` |
| `676571da7a17b0859bb150f010cc258c905d420deb08d4b4339e209ced894b5e` | 3007 | `task-1-report.md` |
| `2cfac707a76bf096c869ea0c2330ed53b79b2b7e4230b9017cb544a14778d667` | 3634 | `task-10-brief.md` |
| `3030bf73161ccd93c63f7f4d887b01bc8cf005f28d00ac7c078ddec4158ac0f4` | 13351 | `task-10-report.md` |
| `17b6724ebad0c7a0ab70c094fc183bd6111015af6840f3f8032c3230b551b524` | 2450 | `task-11-brief.md` |
| `6b1a502be059f9228ea8f5cf2c00edfdcec2489c997c6dfed3b3ba0cf5a1b63e` | 5110 | `task-11-report.md` |
| `f1ac918ebf98a082ad2bcf2304a6e559f59fa676631e3736f15d6a01a6e8b2ac` | 3395 | `task-2-brief.md` |
| `ddaa508bd86fc1f2e5660fd52e94041588b19461e7d6f6793b307447da35076d` | 3565 | `task-2-report.md` |
| `7b0ca5ec220ffaa27705abf36ba9723823622e97a911cb036181f57c0dad7708` | 4750 | `task-3-brief.md` |
| `42724028416253c53807daf945e2da4634bca58100f9bff2b9e0bcf7e9d73d69` | 3491 | `task-3-report.md` |
| `8f1b92c7253d85187f9631875b0a7aeb68977887b162b7a0436cef2c2fdb3ac7` | 9001 | `task-4-brief.md` |
| `840ca308702d0b5ba89a7f974539bd3e1a0d95b32b36c9fbb86a770317b2fa8c` | 16078 | `task-4-report.md` |
| `b57f395f5de718f2d5de141f85b38733557a0b76d1e1d5ee6fdd932d2ac0f6de` | 8419 | `task-5-brief.md` |
| `c9f73d41123e7b3210e847c9427d2bf72b4bf791d7bc64f38ad3399eb0ec1913` | 26626 | `task-5-report.md` |
| `d6c72a80de459efb93e45119e21d908950a9aa32aa201f497fed96f331fbed37` | 7852 | `task-6-brief.md` |
| `8c9eb10a653e92cfb3adba0ae603e25f794f602fe8988a93570e03232f1b6b5d` | 11011 | `task-6-report.md` |
| `f61d31fa4ca96791d1f009875180a734f6150301621e8f161080dc9a8dd91787` | 7934 | `task-7-brief.md` |
| `c1e777bb1c0ca2a140073735825e1544a7a46bb1dcb185006312b16f619ba1aa` | 5329 | `task-7-report.md` |
| `652623e353dda07dbb0dd055b0b3c24d7f336d72bdc0e39e4218adc350403728` | 4942 | `task-8-brief.md` |
| `8421eb77aede354a584955e8212f8b747279b218c2a937e17d0f92543a5c5b27` | 41243 | `task-8-report.md` |
| `2fac7dac5a497ce39e2e0804993bc082cb8026dd029aad978fe7f7eaa7ae8b1f` | 7066 | `task-9-brief.md` |
| `f16b1266dc894b8b812f66ee2015e58f0b5d1be0bfe368b94c86aaad3b4c54cc` | 23413 | `task-9-report.md` |

**Total:** 50 arquivos, 2 418 689 bytes.

## Fora da tabela: o trabalho não commitado de wt2

Preservado no mesmo lugar, em
`docs/superpowers/sdd/2026-09-06-caminho-entre-pares/nao-commitado-em-wt2/wt2-nao-commitado.patch`:

| sha256 | bytes | arquivo |
|---|---:|---|
| `23c031bafbc83dcac4f49fb293172300d338b6f2db34c9837efb6171ae955a30` | 18736 | `wt2-nao-commitado.patch` |

Ele está numa subpasta, e **não** na tabela acima, porque não é um relatório: é
código em andamento de outra sessão, salvo do mesmo `/private/tmp` pelo mesmo
motivo. Não entrou na integração. O relatório ao lado explica o que ele conserta
e por que a decisão de deixá-lo de fora é de quem for mergear, e não desta
tarefa.
