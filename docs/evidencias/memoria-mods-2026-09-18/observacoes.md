# Registro da execução pela interface

Data local: 18/09/2026, aproximadamente 22:47–23:00 (America/Sao_Paulo).

- App instalado aberto pela automação nativa. Info.plist: 0.11.2. Release público associa essa versão ao commit e90bcc16751cad0861c73c3f57bfaa78c5b3e434; SHA-256 do binário local em ambiente.json. Não houve recompilação ou substituição do aplicativo.
- Criado pela UI o servidor separado QA MEMORIA MODS 18-09, inicialmente sem MODs. Um participante local, fora da sala de voz. Nenhum convite enviado.
- Primeira medição sem MODs: sessão padrão, nenhum editor aberto.
- Ativados somente ESTILO 1.0.1, MESA 1.2.1 e PERFIS 1.2.2; conjunto salvo e cliente reconectado. Conferidos na UI os três acessos, o cartão de PERFIS e no banco de teste os três registros enabled=1.
- Duas janelas de coleta com os três carregados, sem abrir seus editores.
- Criada campanha de teste na MESA, sistema D&D 5e 2014, um mapa 20 × 14 sem imagem e uma peça livre. Mapa permaneceu em prévia privada. Uma rolagem 1d20 retornou 11 e persistiu no servidor. Tentativa de arraste não alterou a posição gravada; não foi contabilizada como funcionalidade validada.
- Coleta com a MESA aberta sobre esse tabuleiro, após a interação. Não é uma medição contínua de arraste ou de FPS. Sem trilha sonora, vídeo ou arquivos grandes.
- Aberto ESTILO, escolhida AURORA e salvo o tema no servidor de teste. UI confirmou o salvamento e o tema roxo foi visto na sessão.
- Aberto PERFIS e seu formulário de edição; nenhum dado pessoal editado ou arquivo enviado.
- Saída pela UI confirmada às 22:53:40. Aos 22:54:27, a tela inicial ainda mostrava as cores roxas. Servidor local já não escutava na porta UDP 8383. A queda de memória não significou restauração visual completa.
- Fechado o app com Cmd+Q; todos os quatro processos medidos desapareceram. Reaberto o mesmo binário e hospedado o mesmo servidor, com os três MODs desde a entrada. Nova família de PIDs registrada em ambiente.json. Nenhum editor aberto nessa rodada; tema AURORA e campanha de teste já persistidos.

Os snapshots visuais e de acessibilidade foram inspecionados durante a execução. Este registro não é um dump de heap e não demonstra ausência ou presença de todos os timers e objetos retidos.

- Terceira abertura do mesmo binário: os três MODs foram desativados pela gestão do servidor, o app foi encerrado e reaberto. Reentrada no mesmo servidor confirmou ausência dos acessos de MODs e retorno da aparência laranja. Nenhum registro enabled=1 no banco. Coleta final sem MODs após espera explícita de 20 segundos. A segunda rodada com MODs também teve essa espera.
