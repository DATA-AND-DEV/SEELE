# Estudos de interface

## Fluxo de MODs

`telas-mods.html` é um protótipo navegável para as lacunas abertas pelo ADR 0045:

1. aceite da lista antes da entrada;
2. leitura das notas assinadas de uma versão;
3. administração dos MODs por quem hospeda;
4. falha isolada sem derrubar a sessão.

Abra o arquivo num navegador e use a régua no canto inferior esquerdo. Também é
possível abrir um estado diretamente com `?screen=consent`, `detail`, `manage`
ou `failure`. Acrescente `&capture=1` para ocultar a régua do protótipo.

Os nomes e conteúdos dos MODs são dados de composição, não catálogo real. As
regras de produto, os níveis de avaliação, os alcances e a linguagem vêm do ADR
0045 e da estética vigente em `specs/07-estetica.md`.

## Launcher e versões lado a lado

`telas-launcher.html` cobre o ADR 0046:

1. entrada que usa a versão mais nova sem perguntar;
2. escolha explícita de versão ao hospedar;
3. download da versão exigida por um servidor antigo;
4. gerenciamento do espaço ocupado por apps e dados de cada versão;
5. bloqueio explicativo de uma versão revogada.

Os estados são `home`, `host`, `download`, `versions` e `revoked`, escolhidos
pela régua inferior ou pelo parâmetro `?screen=`. As versões, tamanhos, servidores
e identificador da falha são dados fictícios de composição. Acrescente
`&capture=1` para ocultar a régua.
