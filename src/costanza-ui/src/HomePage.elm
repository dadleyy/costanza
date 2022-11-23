module HomePage exposing (..)

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


type alias HomePage =
    { lastRequest : Request
    , currentInput : String
    , history : List SS.StateHistoryEntry
    , connection : HomeConnectionState
    , lastConnectionMillis : Int
    , lastError : Maybe String
    , tick : Int
    }


type HomeConnectionState
    = Disconnected
    | Websocket Bool


type Message
    = AttemptSend String
    | Noop
    | FileUploadResult (Result Http.Error ())
    | GotFiles (List File.File)
    | UpdateHomeInput String
    | KeyDown Int
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

        KeyDown 13 ->
            ( consumeInput home home.currentInput
            , sendInputMessage home.currentInput home.tick
            )

        KeyDown _ ->
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
    let
        isDisabled =
            homePage.connection
                == Disconnected
                || homePage.lastRequest
                == Pending
                || homePage.connection
                == Websocket False
    in
    Html.div [ AT.class "pt-8 flex items-center flex-col w-full h-full" ]
        [ case homePage.lastError of
            Nothing ->
                Html.div [] []

            Just e ->
                Html.div [ AT.class "error-container mb-4" ] [ Html.text e ]
        , Html.div [ AT.class "flex items-center w-full px-8" ]
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
                , onKeyUp KeyDown
                , AT.disabled isDisabled
                ]
                []
            , Button.view
                ( Button.Icon Button.Plane (AttemptSend homePage.currentInput)
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
                    , Button.view ( Button.Icon Button.File Noop, Button.disabledOr isDisabled Button.Secondary )
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
    in
    case parsed of
        Ok (SS.Response stuff) ->
            ( { home | lastRequest = Done (Ok ()) }, Cmd.none )

        Ok (SS.State state) ->
            let
                nextConnection =
                    if state.serialAvailable then
                        Websocket True

                    else
                        Websocket False
            in
            ( { home | history = state.history, connection = nextConnection }, Cmd.none )

        Err error ->
            ( { home | lastError = Just (JD.errorToString error) }, Cmd.none )


filesDecoder : JD.Decoder (List File.File)
filesDecoder =
    JD.at [ "target", "files" ] (JD.list File.decoder)
