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

    // This function is responsible for connecting to, and adding our websocket event listeners
    // that will send messages into the elm runtime via its `ports`.
    function connect(url: string): void {
      if (websocket) {
        console.warn("closing our current websocket");
        websocket.close();
        websocket = void 0;
      }

      websocket = new WebSocket(url);

      if (!websocket) {
        console.warn("Unable to establish websocket connection");
        websocket = void 0;
        return;
      }

      websocket.addEventListener("open", () => {
        app.ports.messageReceiver.send(
          JSON.stringify({ kind: "control", connected: true })
        );
      });

      websocket.addEventListener("close", () => {
        app.ports.messageReceiver.send(
          JSON.stringify({ kind: "control", connected: false })
        );
      });

      websocket.addEventListener("message", (event) => {
        console.log("Has message from server - ", event.data);

        app.ports.messageReceiver.send(
          JSON.stringify({ kind: "websocket", payload: event.data as string })
        );
      });
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
