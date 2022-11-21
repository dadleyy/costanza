type Flags = {
  apiRoot: string;
  uiRoot: string;
  loginURL: string;
  logoutURL: string;
  version: string;
};

type Ports = {
  messageReceiver: ElmPublisher<string>;
  sendMessage: ElmSubscriber<string>;
};

declare const Elm: ElmRuntime<Flags, Ports>;

type TElmMessage = { kind: "websocket"; payload: string } | { kind: "control" };

(function () {
  function metaValue(metaName: string): string | undefined {
    const container = document.querySelector(`meta[name="${metaName}"]`);
    return container ? container.getAttribute("content") ?? void 0 : void 0;
  }

  function boot(): void {
    const apiRoot = metaValue("apiRoot");
    const wsURL = metaValue("wsURL");
    const version = metaValue("version");
    const loginURL = metaValue("loginURL");
    const logoutURL = metaValue("logoutURL");
    const uiRoot = metaValue("uiRoot");

    if (!apiRoot || !version || !loginURL || !uiRoot || !logoutURL || !wsURL) {
      console.error("unable to create elm runtime environment");

      return void 0;
    }

    console.log("booting");
    const flags = { apiRoot, version, loginURL, uiRoot, logoutURL };
    const app = Elm.Main.init({ flags });

    let websocket: WebSocket | undefined = void 0;
    let messageCount = 0;

    // The callback that will be used when our websocket is successfully opened.
    const open = () => {
      app.ports.messageReceiver.send(
        JSON.stringify({ kind: "control", connected: true })
      );
    };

    // The callback that will be used when our websocket errors. removes callbacks.
    const cleanup = () => {
      messageCount = 0;
      app.ports.messageReceiver.send(
        JSON.stringify({ kind: "control", connected: false })
      );

      if (!websocket) {
        return;
      }

      const temp = websocket;
      websocket = void 0;

      console.log("cleaning up websocket");
      try {
        temp.close();
      } catch (error) {
        console.warn("unable to close previous websocket");
      }

      temp.removeEventListener("open", open);
      temp.removeEventListener("close", cleanup);
      temp.removeEventListener("error", cleanup);
    };

    const ondata = (event: MessageEvent) => {
      console.log(`(boot)[${messageCount}] message from server`, {
        data: event.data,
      });
      messageCount += 1;

      app.ports.messageReceiver.send(
        JSON.stringify({ kind: "websocket", payload: event.data as string })
      );
    };

    // This function is responsible for connecting to, and adding our websocket event listeners
    // that will send messages into the elm runtime via its `ports`.
    function connect(url: string): void {
      console.log("attempting to create new websocket");
      if (websocket) {
        cleanup();
      }

      try {
        websocket = new WebSocket(url);
      } catch (error) {
        console.warn("Unable to establish websocket connection", error);
        websocket = void 0;
        return;
      }

      if (!websocket) {
        console.warn("Unable to establish websocket connection");
        websocket = void 0;
        return;
      }

      websocket.addEventListener("open", open);
      websocket.addEventListener("close", cleanup);
      websocket.addEventListener("error", cleanup);
      websocket.addEventListener("message", ondata);
    }

    // This function is used to subscribe to messages received from elm. These will either be bound
    // for the websocket to the server, or are intended for us here in js land.
    function processElmMessage(content: string): void {
      try {
        const parsed: TElmMessage = JSON.parse(content);
        switch (parsed.kind) {
          case "websocket":
            websocket?.send(parsed.payload);
            break;

          // todo: provide actual information inside of here that will control this outer, js
          // layer. currently all this is doing is being a way for elm to connect our ws.
          case "control":
            connect(wsURL || "");
            break;
          default:
            console.warn("Unrecognized elm message", content);
            break;
        }
      } catch (error) {
        console.error("Problem handling elm message", error);
      }
    }

    app.ports.sendMessage.subscribe(processElmMessage);
  }

  window.addEventListener("DOMContentLoaded", boot);
})();
