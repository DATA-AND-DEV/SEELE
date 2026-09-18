# Os MODs saíram desta pasta

Os fontes do **Estilo** e do **Perfis** foram para repositórios próprios, como
o MESA já estava. Esta pasta guarda só este aviso.

| MOD | repositório | no catálogo |
| --- | --- | --- |
| Estilo | <https://github.com/DATA-AND-DEV/ESTILO> | `seele/estilo` |
| Perfis | <https://github.com/DATA-AND-DEV/PERFIS> | `seele/perfis` |
| Mesa | <https://github.com/DATA-AND-DEV/MESA> | `seele/mesa` |

**Por que cada um no seu repositório.** O indexador não copia código: ele fixa
`repo` mais `commit`, e busca os bytes de lá. Um MOD que morasse aqui dentro
não teria commit próprio a fixar — e o que se publica é sempre o commit que foi
avaliado, nunca o que está na árvore de alguém.

Quem quiser escrever um MOD começa pelo guia, e não por esta pasta:
<https://mods.seele.app.br/guia/>
