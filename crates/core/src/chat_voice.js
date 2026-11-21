(() => {
    class Tail {
        ids = null;
        pending = new Set();

        update(rows) {
            const current = rows.filter(row => row.id);
            const ids = new Set(current.map(row => row.id));
            const anchor = this.ids && current.findLastIndex(row => this.ids.has(row.id));
            const added = this.ids && (anchor >= 0 || this.ids.size === 0)
                ? current.slice(anchor + 1) : [];
            for (const row of added) this.pending.add(row.id);
            const play = [];
            for (const row of current) {
                if (!row.voice || !this.pending.delete(row.id)) continue;
                if (!added.includes(row)) added.push(row);
                if (!row.mine) play.push(row.id);
            }
            for (const id of this.pending) if (!ids.has(id)) this.pending.delete(id);
            this.ids = ids;
            return {added, play};
        }
    }

    if (typeof module === 'object' && typeof window === 'undefined') {
        module.exports = {Tail};
        return;
    }

    const F = window.__fb;
    const tail = new Tail();
    const queue = [];
    const origin = 'https://www.facebook.com';
    let thread, active, composer;
    const log = () => document.querySelector('[role=log]');
    const rows = () => [...(log()?.querySelectorAll('[role=article]') || [])];
    const label = e => (e.getAttribute('aria-label') || e.getAttribute('title') || e.innerText || '').trim();
    const button = (root, pattern) => [...(root?.querySelectorAll('button,[role=button]') || [])]
        .find(e => F.visible(e) && F.enabled(e) && pattern.test(label(e)));
    const PLAY = /^(play|lecture|lire|phát|воспроизвести|přehrát|prehrať|esita|reproducir|abspielen|riproduci|reproduzir)(?:\s+(?:audio|voice message))?$/i;
    const PAUSE = /^(pause|mettre en pause|tạm dừng|пауза|приостановить|pozastavit|pozastaviť|peata|pausar|pausa)(?:\s+(?:audio|voice message))?$/i;
    const slider = row => row.querySelector('[role=slider][aria-valuemax]');
    const isVoice = row => !!(slider(row) && (button(row, PLAY) || button(row, PAUSE)));
    const voiceId = row => row.querySelector('[data-message-id]')?.getAttribute('data-message-id');
    const decorate = (m, voice) => ({
        id: m.id || '',
        time: m.t || '',
        sender: m.from || '?',
        mine: m.mine === true || m.from === 'me',
        text: m.text || '',
        attachment: m.att || '',
        reactions: m.re || '',
        voice,
    });
    const snapshot = () => rows().map(row => decorate(F.parseMessage(row, log()), isVoice(row)));
    const pathname = () => location.pathname.replace(/\/$/, '');
    const START = /^(send a voice clip|envoyer un (?:clip|message) (?:audio|vocal)|gửi đoạn thoại|отправить голосовое сообщение|odeslat hlasovou zprávu|poslať hlasovú správu|saada häälsõnum|enviar (?:un )?mensaje de voz|sprachnachricht senden|invia (?:un )?messaggio vocale)$/i;
    const STOP = /^(stop recording|arrêter l[’']enregistrement|dừng ghi âm|остановить запись|zastavit nahrávání|zastaviť nahrávanie|lõpeta salvestamine|detener grabación|aufnahme stoppen|interrompi registrazione)$/i;
    const CANCEL = /^(cancel|annuler|hủy|отмена|zrušit|zrušiť|tühista|cancelar|abbrechen|annulla)$/i;
    const SEND = /^(press enter to send|appuyez sur entrée pour envoyer|nhấn enter để gửi|нажмите enter, чтобы отправить|stisknutím klávesy enter odešlete|vajuta saatmiseks enterit|presiona enter para enviar|zum senden die eingabetaste drücken|premi invio per inviare)$/i;
    const MIC_DIALOG = /^(allow facebook to access your microphone|autoriser facebook à accéder à votre microphone|cho phép facebook truy cập micrô của bạn|разрешите facebook доступ к микрофону)$/i;
    const recording = () => button(composer, STOP);
    const draft = () => button(composer, SEND) && button(composer, CANCEL);
    const recorder = () => recording() ? 'recording' : draft() ? 'ready' : 'idle';
    const stopPlayback = () => {
        if (active?.isConnected) button(active, PAUSE)?.click();
        active = null;
        queue.length = 0;
    };

    async function play(row) {
        const playButton = button(row, PLAY);
        if (!playButton) return false;
        playButton.click();
        active = row;
        return !!await F.waitFor(() => button(row, PAUSE), 4000, 100);
    }

    async function read(count) {
        const voices = new Set();
        const collect = () => {
            for (const row of rows()) {
                const id = voiceId(row);
                if (id && isVoice(row)) voices.add(id);
            }
        };
        const merge = F.mergeChunks;
        const settle = F.settleBottom;
        F.mergeChunks = (...chunks) => {
            collect();
            return merge(...chunks);
        };
        F.settleBottom = async (...args) => {
            const result = await settle(...args);
            collect();
            if (!tail.ids) tail.update(snapshot());
            return result;
        };
        try {
            collect();
            const output = await F.read(count);
            if (!tail.ids) tail.update(snapshot());
            return {
                rows: output.msgs.map(m => m.d ? {date: m.d} : decorate(m, voices.has(m.id))),
                complete: count >= 1000000 ? output.complete : null,
            };
        } finally {
            F.mergeChunks = merge;
            F.settleBottom = settle;
        }
    }

    async function pulse() {
        const playing = active?.isConnected && button(active, PAUSE);
        if (!playing) active = null;
        const scroller = log() && F.scroller(log());
        if (scroller && scroller.scrollTop + scroller.clientHeight < scroller.scrollHeight - 10) {
            if (!playing) scroller.scrollTop = scroller.scrollHeight;
            return {};
        }
        const current = snapshot();
        const {added, play: incoming} = tail.update(current);
        queue.push(...incoming);
        let error;
        if (!playing && recorder() === 'idle' && queue.length) {
            const next = queue.shift();
            const row = rows().find(row => voiceId(row) === next);
            if (row && !await play(row)) error = 'play';
        }
        return {rows: added, error};
    }

    window.__blindfbChatVoice = async (action, value) => {
        if (action === 'open') {
            const sameId = pathname().split('/').pop() === value.split('/').pop();
            if (location.origin !== origin || !sameId || !/^\/messages\/(?:e2ee\/)?t\/\d+$/.test(pathname()))
                return {error: 'thread'};
            const expected = pathname();
            if (!await F.threadReady()) return {error: 'locked'};
            if (pathname() !== expected) return {error: 'thread'};
            thread = pathname();
            return {};
        }
        if (location.origin !== origin || pathname() !== thread) return {error: 'thread'};
        switch (action) {
        case 'read': return read(value);
        case 'pulse': return {...await pulse(), recorder: recorder()};
        case 'play': {
            if (recorder() !== 'idle') return {error: 'recording'};
            const wasPlaying = active && voiceId(active) === value && button(active, PAUSE);
            stopPlayback();
            if (wasPlaying) return {};
            let row = rows().find(row => voiceId(row) === value);
            const sc = log() && F.scroller(log());
            const until = Date.now() + 30000;
            while (!row && sc && Date.now() < until) {
                const top = sc.scrollTop;
                sc.scrollTop = Math.max(0, top - Math.max(100, sc.clientHeight * 0.8));
                await F.sleep(300);
                row = rows().find(row => voiceId(row) === value);
                if (sc.scrollTop === top) break;
            }
            return row && await play(row) ? {} : {error: 'play'};
        }
        case 'stop-playback':
            stopPlayback();
            return {};
        case 'permission':
            return {permission: (await navigator.permissions.query({name: 'microphone'})).state};
        case 'start': {
            if (recorder() !== 'idle') return {error: 'recording', recorder: recorder()};
            stopPlayback();
            const start = button(document, START);
            composer = start?.closest('[role=region]');
            if (!composer) return {error: 'no-recorder'};
            start.click();
            const ready = await F.waitFor(() => recording() || [...document.querySelectorAll('[role=dialog]')]
                .find(d => F.visible(d) && MIC_DIALOG.test(label(d))), 5000, 100);
            if (ready?.matches('[role=dialog]')) button(ready, /^(ok|okay)$/i)?.click();
            await F.waitFor(recording, 5000, 100);
            return {recorder: recorder(), error: recording() ? undefined : 'microphone'};
        }
        case 'stop':
            recording()?.click();
            await F.waitFor(() => !recording(), 3000, 100);
            return {recorder: recorder(), error: recorder() === 'ready' ? undefined : 'recording'};
        case 'discard':
            button(composer, CANCEL)?.click();
            await F.waitFor(() => recorder() === 'idle', 3000, 100);
            return {recorder: recorder(), error: recorder() === 'idle' ? undefined : 'recording'};
        case 'send': {
            if (recorder() !== 'ready') return {error: 'recording', recorder: recorder()};
            const before = new Set(snapshot().map(row => row.id));
            button(composer, SEND).click();
            const sent = await F.waitFor(() => snapshot()
                .some(row => row.id && row.voice && row.mine && !before.has(row.id)), 12000, 150);
            return {recorder: recorder(), error: sent ? undefined : 'send'};
        }
        }
    };
})();
