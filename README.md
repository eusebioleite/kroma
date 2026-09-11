# Kroma

![Rust](https://img.shields.io/badge/Rust-2024-orange?logo=rust&logoColor=white)
![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)
![Oracle](https://img.shields.io/badge/Oracle-DB-red?logo=oracle&logoColor=white)
![SMTP](https://img.shields.io/badge/SMTP-TLS%20%2F%20STARTTLS-green)

> Serviço de despacho de e-mails via fila Oracle — substituto leve e eficiente para o DebxMail.

O **Kroma** é um daemon escrito em Rust que monitora uma fila de e-mails armazenada em um banco Oracle (`A_MAIL_QUEUE` e `A_MAIL_ANEX`) e os envia de forma assíncrona via SMTP. Ele foi criado para substituir o DebxMail com uma solução mais leve e eficiente.

---

## Funcionalidades

- 🔁 **Polling configurável** — verifica a fila em intervalos definidos por você
- 📬 **Envio assíncrono** — processa múltiplos e-mails em sequência com throttle ajustável
- 🔒 **TLS e STARTTLS** — escolha automática baseada na porta configurada (465 = TLS, outros = STARTTLS)

---

## Pré-requisitos

- [Rust](https://rustup.rs/) (edição 2024)
- [Oracle Instant Client](https://www.oracle.com/database/technologies/instant-client.html) instalado e configurado no `PATH` / `LD_LIBRARY_PATH`
  - **Importante:** Defina a variável de ambiente `OCI_LIB_DIR` apontando para o diretório das bibliotecas do Oracle SDK:
    - **Windows:** `C:\Oracle\instantclient_21_13\sdk\lib\msvc`
- Acesso a um banco Oracle com as tabelas `A_MAIL_QUEUE` e `A_MAIL_ANEX` (estrutura padrão do Debx)
- Servidor SMTP acessível (com suporte a TLS ou STARTTLS)

---

## Instalação

```bash
git clone https://github.com/seu-usuario/kroma.git
cd kroma

# Defina a variável OCI_LIB_DIR
set OCI_LIB_DIR=C:\Oracle\instantclient_21_13\sdk\lib\msvc

# Compile o projeto
cargo build --release
```

O binário estará em `target/release/kroma.exe`.

---

## Configuração

Crie um arquivo `kroma.toml` na mesma pasta do binário (use o `kroma.toml.example` como base):

```toml
[credentials]
user = "email@exemplo.com"
password = "sua_senha"

[server]
host = "smtp.exemplo.com"
port = 587          # 465 para TLS, 587 ou 25 para STARTTLS

[database]
user = "usuario_oracle"
password = "senha_oracle"
host = "host_oracle"
port = 1521
sid = "nome_do_sid"

[service]
interval = 60       # intervalo entre verificações da fila, em segundos
throttle = 100      # pausa entre envios consecutivos, em milissegundos (0 = sem pausa)
```

---

## Uso

### Executar o serviço

```bash
./kroma
```

O Kroma iniciará em loop, verificando a fila a cada `interval` segundos e enviando os e-mails encontrados.

### Modo de teste

Verifica a conectividade com o SMTP e o Oracle e exibe o resultado:

```bash
./kroma -t
```

Envia um e-mail de teste real para um endereço específico:

```bash
./kroma -t destinatario@exemplo.com
```

---

## Como funciona

```
┌─────────────────────────────────────────────────────────┐
│                        Kroma                            │
│                                                         │
│  1. Aguarda o intervalo configurado                     │
│  2. Consulta A_MAIL_QUEUE                               │
│  3. Para cada e-mail:                                   │
│     a. Busca anexos em A_MAIL_ANEX                      │
│     b. Monta e envia via SMTP                           │
│     c. Chama DEBXMAIL.FINAL_ENVIO() com status (S/N)    │
│  4. Volta ao passo 1                                    │
└─────────────────────────────────────────────────────────┘
```

O Kroma lê diretamente das tabelas de fila do Debx (`A_MAIL_QUEUE` para os e-mails e `A_MAIL_ANEX` para os anexos) e reporta o resultado de cada envio chamando a procedure `CONSOLIDADO.DEBXMAIL.FINAL_ENVIO()`.

---

## Logs

Os logs são gerados em formato JSON na pasta `logs/` com rotação diária:

```
logs/
  kroma.log         # arquivo do dia atual
  kroma.log.2026-09-10  # arquivos anteriores
```

---

## Licença

Distribuído sob a licença [MIT](LICENSE).

---

> **Nota:** O Kroma integra-se com tabelas e procedures do sistema Debx (Zucchetti), mas não distribui nem contém código proprietário desse sistema. Toda a lógica de envio e integração é independente e implementada nativamente em Rust.
