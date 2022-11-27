module HomePage exposing (HomePage, Message(..), init, update, view)

import Boot
import Button
import Environment as Env
import File
import Html
import Html.Attributes as AT
import Html.Events as EV
import Http
import Icon
import Json.Decode as JD
import Json.Encode as JE
import StateSync as SS
import Time


type Request
    = Done (Result Http.Error ())
    | Pending
    | NotAsked


type alias HomePageConfigurationView =
    { device : String
    , baud : Int
    , lastAttempt : Request
    }


type HomePageView
    = Terminal
    | Configure HomePageConfigurationView


type alias HomePage =
    { lastRequest : Request
    , currentInput : String
    , history : List SS.StateHistoryEntry
    , connection : HomeConnectionState
    , lastConnectionMillis : Int
    , lastError : Maybe String
    , view : HomePageView
    , tick : Int
    }


type HomeConnectionState
    = Disconnected
    | Websocket Bool


type InputKind
    = TerminalInputKeyUp
    | ConfigurationFormKeyup


type Message
    = AttemptSend String
    | Noop
    | SubmitConfig
    | ToggleView
    | UpdateDevice String
    | UpdateBaud String
    | FileUploadResult (Result Http.Error ())
    | GotFiles (List File.File)
    | UpdateHomeInput String
    | KeyUp InputKind Int
    | RawWebsocket String
    | Tick Time.Posix


type alias WebsocketResponse =
    { kind : String
    , connected : Maybe Bool
    , ok : Maybe Bool
    , payload : Maybe String
    }


init : HomePage
init =
    { lastRequest = NotAsked
    , tick = 0
    , history = []
    , view = Terminal
    , connection = Disconnected
    , currentInput = ""
    , lastConnectionMillis = 0
    , lastError = Nothing
    }


uploadUrl : Env.Environment -> String
uploadUrl env =
    env.apiRoot ++ "/upload"


upload : Env.Environment -> File.File -> Cmd Message
upload env file =
    Http.post { body = Http.fileBody file, expect = Http.expectWhatever FileUploadResult, url = uploadUrl env }


update : Message -> ( HomePage, Env.Environment ) -> ( HomePage, Cmd Message )
update message ( home, env ) =
    case message of
        UpdateDevice newDevice ->
            let
                newHome =
                    { home
                        | view =
                            case home.view of
                                Configure configuration ->
                                    Configure
                                        { configuration | device = newDevice }

                                _ ->
                                    home.view
                    }
            in
            ( newHome, Cmd.none )

        UpdateBaud newBaud ->
            let
                newHome =
                    { home
                        | view =
                            case home.view of
                                Configure configuration ->
                                    Configure
                                        { configuration | baud = String.toInt newBaud |> Maybe.withDefault 0 }

                                _ ->
                                    home.view
                    }
            in
            ( newHome, Cmd.none )

        SubmitConfig ->
            sendConfig home

        ToggleView ->
            let
                next =
                    case home.view of
                        Configure _ ->
                            Terminal

                        Terminal ->
                            Configure { device = "", baud = 0, lastAttempt = NotAsked }
            in
            ( { home | view = next }, Cmd.none )

        FileUploadResult _ ->
            ( home, Cmd.none )

        GotFiles list ->
            let
                cmds =
                    List.map (upload env) list
            in
            ( home, Cmd.batch cmds )

        Noop ->
            ( home, Cmd.none )

        KeyUp TerminalInputKeyUp 13 ->
            sendConfig home

        KeyUp ConfigurationFormKeyup 13 ->
            sendConfig home

        KeyUp _ _ ->
            ( home, Cmd.none )

        Tick posixValue ->
            let
                nowMillis =
                    Time.posixToMillis posixValue

                diff =
                    nowMillis - home.lastConnectionMillis

                -- During tick, check reconnection here.
                ( nextHome, cmd ) =
                    case ( home.connection, diff > 5000 ) of
                        ( Disconnected, True ) ->
                            ( { home | lastConnectionMillis = nowMillis }, Boot.startWebsocket )

                        _ ->
                            ( home, Cmd.none )
            in
            ( nextHome, cmd )

        RawWebsocket outerPayload ->
            let
                parsed =
                    parseMessage outerPayload
            in
            case parsed of
                Ok parsedMessage ->
                    case ( parsedMessage.kind, parsedMessage.connected ) of
                        ( "control", Just True ) ->
                            ( { home | connection = Websocket False }, Cmd.none )

                        ( "control", Just False ) ->
                            ( { home | connection = Disconnected, history = [] }, Cmd.none )

                        ( "response", _ ) ->
                            ( { home | lastRequest = Done (Ok ()) }, Cmd.none )

                        ( "websocket", _ ) ->
                            Maybe.withDefault
                                ( home, Cmd.none )
                                (Maybe.map (handleInnerWebsocketMessage home) parsedMessage.payload)

                        _ ->
                            ( home, Cmd.none )

                -- TODO: we were unable to parse a websocket message.
                Err error ->
                    ( { home | lastError = Just (JD.errorToString error) }, Cmd.none )

        AttemptSend payload ->
            ( consumeInput home payload, sendInputMessage payload home.tick )

        UpdateHomeInput value ->
            ( { home | currentInput = value }, Cmd.none )


consumeInput : HomePage -> String -> HomePage
consumeInput home payload =
    let
        newTick =
            home.tick + 1

        newHistory =
            List.append home.history [ SS.SentCommand payload ]
    in
    { home | tick = newTick, lastRequest = Pending, currentInput = "", history = newHistory }


onKeyUp : (Int -> msg) -> Html.Attribute msg
onKeyUp tagger =
    EV.on "keyup" (JD.map tagger EV.keyCode)


view : HomePage -> Html.Html Message
view homePage =
    Html.div [ AT.class "pt-8 flex items-center flex-col w-full h-full" ]
        [ case homePage.lastError of
            Nothing ->
                Html.div [] []

            Just e ->
                Html.div [ AT.class "error-container mb-4" ] [ Html.text e ]
        , viewTabs homePage.view
        , case homePage.view of
            Configure configuration ->
                viewConfiguration homePage configuration

            Terminal ->
                viewTerminal homePage
        ]


viewConfiguration : HomePage -> HomePageConfigurationView -> Html.Html Message
viewConfiguration home config =
    Html.div [ AT.class "flex items-center w-full px-8" ]
        [ Html.div [ AT.class "mr-4 flex-1" ]
            [ Html.input
                [ AT.type_ "text"
                , AT.class "block w-full"
                , AT.value config.device
                , EV.onInput UpdateDevice
                , onKeyUp (KeyUp ConfigurationFormKeyup)
                , AT.placeholder "Device"
                , AT.disabled (config.lastAttempt == Pending)
                ]
                []
            ]
        , Html.div [ AT.class "mr-4 flex-1" ]
            [ Html.input
                [ AT.type_ "number"
                , AT.class "block w-full"
                , EV.onInput UpdateBaud
                , onKeyUp (KeyUp ConfigurationFormKeyup)
                , AT.value (String.fromInt config.baud)
                , AT.placeholder "Baud"
                , AT.disabled (config.lastAttempt == Pending)
                ]
                []
            ]
        , Button.view
            ( Button.Icon Icon.Plane SubmitConfig
            , if config.lastAttempt == Pending then
                Button.Disabled

              else
                Button.Primary
            )
        ]


viewTerminal : HomePage -> Html.Html Message
viewTerminal homePage =
    let
        isDisabled =
            homePage.connection
                == Disconnected
                || homePage.lastRequest
                == Pending
                || homePage.connection
                == Websocket False
    in
    Html.div [ AT.class "w-full relative" ]
        [ Html.div [ AT.class "flex items-center w-full px-8" ]
            [ Html.div [ AT.class "mr-4" ]
                [ case homePage.connection of
                    Disconnected ->
                        Html.div [ AT.class "text-slate-100" ] [ Icon.view Icon.Exclamation ]

                    Websocket False ->
                        Html.div [ AT.class "text-slate-100" ] [ Icon.view Icon.Circle ]

                    Websocket True ->
                        Html.div [ AT.class "text-slate-100" ] [ Icon.view Icon.CircleFull ]
                ]
            , Html.input
                [ AT.type_ "text"
                , AT.class "mr-3 flex-1"
                , AT.value homePage.currentInput
                , EV.onInput UpdateHomeInput
                , onKeyUp (KeyUp TerminalInputKeyUp)
                , AT.disabled isDisabled
                ]
                []
            , Button.view
                ( Button.Icon Icon.Plane (AttemptSend homePage.currentInput)
                , Button.disabledOr (String.isEmpty homePage.currentInput || isDisabled) Button.Primary
                )
            , Html.div [ AT.class "ml-4 relative" ]
                [ Html.label [ AT.class "relative block" ]
                    [ Html.input
                        [ AT.class "absolute w-full h-full inset-0 opacity-0"
                        , AT.type_ "file"
                        , AT.disabled isDisabled
                        , AT.accept "text/x.gcode, text"
                        , EV.on "change" (JD.map GotFiles filesDecoder)
                        ]
                        []
                    , Button.view ( Button.Icon Icon.File Noop, Button.disabledOr isDisabled Button.Secondary )
                    ]
                ]
            ]
        , Html.div [ AT.class "w-full flex-1 px-8 mt-4" ]
            [ Html.div [ AT.class "code-container w-full" ]
                [ Html.code [ AT.class "scrollback-terminal" ]
                    (List.map renderHistoryEntry (List.reverse homePage.history))
                ]
            ]
        ]


renderHistoryEntry : SS.StateHistoryEntry -> Html.Html Message
renderHistoryEntry entry =
    case entry of
        SS.SentCommand message ->
            Html.div [ AT.class "flex items-center" ]
                [ Html.div [ AT.class "mr-4" ] [ Icon.view Icon.ChevronLeft ]
                , Html.div [] [ Html.text message ]
                ]

        SS.ReceivedData message ->
            Html.div [ AT.class "flex items-center" ]
                [ Html.div [ AT.class "mr-4" ] [ Icon.view Icon.ChevronRight ]
                , Html.div [] [ Html.text message ]
                ]


sendInputMessage : String -> Int -> Cmd Message
sendInputMessage input tick =
    let
        -- Encode the actual command as a json value, serialize that into a string, and encode that string
        -- into another json value. The intermediary json value is what is parsed on the JS/TS `boot`
        -- "kernel" that is sent along to the server.
        payload =
            JE.object [ ( "kind", JE.string "raw_serial" ), ( "value", JE.string input ), ( "tick", JE.int tick ) ]

        values =
            JE.object [ ( "kind", JE.string "websocket" ), ( "payload", JE.string (JE.encode 0 payload) ) ]
    in
    Boot.sendMessage (JE.encode 0 values)


decoder : JD.Decoder WebsocketResponse
decoder =
    JD.map4 WebsocketResponse
        (JD.field "kind" JD.string)
        (JD.maybe (JD.field "connected" JD.bool))
        (JD.maybe (JD.field "ok" JD.bool))
        (JD.maybe (JD.field "payload" JD.string))


parseMessage : String -> Result JD.Error WebsocketResponse
parseMessage inner =
    JD.decodeString decoder inner


handleInnerWebsocketMessage : HomePage -> String -> ( HomePage, Cmd Message )
handleInnerWebsocketMessage home message =
    let
        parsed =
            SS.parseMessage message

        -- todo: here we are saying that any time we recieve a websocket message from the server,
        -- if we are currently configuring and waiting for a response, we stay on that page. Otherwise,
        -- we automatically exchange ourselves for the terminal view.
        nextView =
            case home.view of
                Configure configurationState ->
                    let
                        attemptState =
                            case ( configurationState.lastAttempt, parsed ) of
                                ( Pending, Ok (SS.State state) ) ->
                                    Done (Ok ())

                                _ ->
                                    configurationState.lastAttempt

                        goToTerminal =
                            case attemptState of
                                Done (Ok _) ->
                                    True

                                _ ->
                                    False
                    in
                    if goToTerminal then
                        Terminal

                    else
                        Configure { configurationState | lastAttempt = attemptState }

                Terminal ->
                    Terminal
    in
    case parsed of
        Ok (SS.Response stuff) ->
            ( { home | lastRequest = Done (Ok ()), view = nextView }, Cmd.none )

        Ok (SS.State state) ->
            let
                nextConnection =
                    if state.serialAvailable then
                        Websocket True

                    else
                        Websocket False
            in
            ( { home | history = state.history, view = nextView, connection = nextConnection }, Cmd.none )

        Err error ->
            ( { home | lastError = Just (JD.errorToString error), view = nextView }, Cmd.none )


filesDecoder : JD.Decoder (List File.File)
filesDecoder =
    JD.at [ "target", "files" ] (JD.list File.decoder)


viewTabs : HomePageView -> Html.Html Message
viewTabs page =
    let
        isConfig =
            case page of
                Configure _ ->
                    True

                _ ->
                    False
    in
    Html.div [ AT.class "flex items-center mb-5" ]
        [ Html.div [ AT.class "mr-5" ]
            [ Button.view
                ( Button.Icon Icon.Terminal ToggleView
                , if page == Terminal then
                    Button.Disabled

                  else
                    Button.Secondary
                )
            ]
        , Html.div []
            [ Button.view
                ( Button.Icon Icon.Configuration ToggleView
                , if isConfig then
                    Button.Disabled

                  else
                    Button.Secondary
                )
            ]
        ]


sendConfig : HomePage -> ( HomePage, Cmd Message )
sendConfig home =
    let
        ( nextHome, cmd ) =
            case home.view of
                Configure config ->
                    ( { home | view = Configure { config | lastAttempt = Pending } }, submitConfig config )

                _ ->
                    ( home, Cmd.none )
    in
    ( nextHome, cmd )


submitConfig : HomePageConfigurationView -> Cmd Message
submitConfig config =
    let
        payload =
            JE.object
                [ ( "kind", JE.string "configuration" )
                , ( "device", JE.string config.device )
                , ( "baud", JE.int config.baud )
                ]

        values =
            JE.object [ ( "kind", JE.string "websocket" ), ( "payload", JE.string (JE.encode 0 payload) ) ]
    in
    Boot.sendMessage (JE.encode 0 values)
