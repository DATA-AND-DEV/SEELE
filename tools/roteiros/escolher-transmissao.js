// A escolha entre transmissões, exercitada no DOM.
//
// O que os guardas de texto não alcançam: se a fileira some com uma transmissão
// só, se a que está no palco é a marcada, e se a própria transmissão de quem
// compartilha fica de fora da escolha.
//
// `telaEmCurso` é escrito por `abrirImagemDaTela`, quando um fluxo abre de
// verdade. Aqui ele é posto à mão porque o que se mede é o desenho, e abrir um
// fluxo pediria um servidor.

const roster = [
  { id: 10, nickname: "rafa" },
  { id: 20, nickname: "marcela" },
  { id: 30, nickname: "eu" },
];

// `voice_rooms` com `occupied_by_us` porque sair da sala esquece a recusa, e
// sem esta chave todo retrato pareceria «fora de sala».
const naSala = [{ id: 1, occupied_by_us: true }];

const semNenhuma = { roster, voice_rooms: naSala, transmissoes: [] };
const soUma = {
  roster,
  voice_rooms: naSala,
  transmissoes: [{ tela: 7, de: 10, e_minha: false }],
};
const duas = {
  roster,
  voice_rooms: naSala,
  transmissoes: [
    { tela: 7, de: 10, e_minha: false },
    { tela: 8, de: 20, e_minha: false },
  ],
};
const umaMinhaUmaAlheia = {
  roster,
  voice_rooms: naSala,
  transmissoes: [
    { tela: 7, de: 10, e_minha: false },
    { tela: 9, de: 30, e_minha: true },
  ],
};

function estado() {
  const onde = $("palco-escolha");
  const rotulos = [...onde.querySelectorAll("button")].map(
    (b) => b.textContent + (b.getAttribute("aria-pressed") === "true" ? "*" : ""),
  );
  return (onde.hidden ? "escondida" : "visivel") + " [" + rotulos.join(" ") + "]";
}

desenharTransmissoes(semNenhuma);
relatar("ninguém transmitindo: " + estado());

desenharTransmissoes(soUma);
relatar("uma só: " + estado());

telaEmCurso = 7;
desenharTransmissoes(duas);
relatar("duas, recebendo a 7: " + estado());

telaEmCurso = 8;
desenharTransmissoes(duas);
relatar("duas, recebendo a 8: " + estado());

telaEmCurso = 7;
desenharTransmissoes(umaMinhaUmaAlheia);
relatar("a minha não entra na escolha: " + estado());

// Um id que o roster não conhece ainda: travessão, e nunca o número cru.
desenharTransmissoes({
  roster,
  voice_rooms: naSala,
  transmissoes: [
    { tela: 7, de: 10, e_minha: false },
    { tela: 8, de: 99, e_minha: false },
  ],
});
relatar("quem o roster não tem: " + estado());

// ---- não querer ver ----
//
// Com uma transmissão só a fileira agora aparece: escolher entre ver e não ver
// é escolha, e era a que não tinha por onde ser feita.

telaEmCurso = 7;
desenharTransmissoes(soUma);
relatar("uma só, recebendo: " + estado());

const naoVer = [...$("palco-escolha").querySelectorAll("button")].at(-1);
relatar("o último botão é: " + naoVer.textContent);
naoVer.click();
await espera(200);
relatar("depois do NÃO VER: telaEmCurso=" + telaEmCurso);

desenharTransmissoes(soUma);
relatar("e a fileira diz: " + estado());

// O servidor liga por conta própria na primeira transmissão da sala. Quem
// recusou não pode ser religado por isso.
await abrirImagemDaTela(7, 1920, 1080);
relatar("o servidor tentou ligar: telaEmCurso=" + telaEmCurso);

// Escolher uma tela desdiz a recusa.
await trocarDeTransmissao(7);
relatar("depois de escolher de novo: naoQueroVer=" + naoQueroVer);

// E sair da sala esquece.
desenharTransmissoes({ roster, voice_rooms: [{ id: 1, occupied_by_us: false }], transmissoes: [] });
relatar("fora da sala: naoQueroVer=" + naoQueroVer + " · fileira " + estado());
