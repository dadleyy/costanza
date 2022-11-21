port module Main exposing (..)

import Browser
import Browser.Navigation as Nav
import Button
import Html
import Html.Attributes as AT
import Html.Events as EV
import Http
import Icon
import Json.Decode as JD
import Json.Encode as JE
import Task
import Time
import Url


type alias Environment =
    { apiRoot : String
    , uiRoot : String
    , loginURL : String
    , logoutURL : String
    , version : String
    }


type alias SessionUserData =
    { name : String
    , picture : String
    , user_id : String
    }


type alias SessionData =
    { user : Maybe SessionUserData }


type alias SessionPayload =
    { ok : Bool
    , session : SessionData
    }


type Request
    = Done (Result Http.Error ())
    | Pending
    | NotAsked


type alias HomePage =
    { lastRequest : Request
    , currentInput : String
    , connected : Bool
    , tick : Int
    }


type Page
    = Home HomePage


type Model
    = Booting Environment
    | Unauthorized Environment
    | Authorized Page Environment SessionUserData
    | Failed Environment


type Message
    = LinkClicked Browser.UrlRequest
    | UrlChanged Url.Url
    | UpdateHomeInput String
    | SessionLoaded (Result Http.Error SessionPayload)
    | WebsocketMessage String
    | AttemptSend String
    | Tick Time.Posix


type alias WebsocketResponse =
    { kind : String
    , connected : Maybe Bool
    , payload : Maybe String
    }


port sendMessage : String -> Cmd msg


port messageReceiver : (String -> msg) -> Sub msg


main : Program Environment Model Message
main =
    Browser.application
        { init = init
        , view = view
        , update = update
        , subscriptions = subscriptions
        , onUrlChange = UrlChanged
        , onUrlRequest = LinkClicked
        }


init : Environment -> Url.Url -> Nav.Key -> ( Model, Cmd Message )
init env url key =
    let
        model =
            Booting env
    in
    ( model, Cmd.batch [ loadAuth model ] )


update : Message -> Model -> ( Model, Cmd Message )
update message model =
    let
        env =
            envFromModel model
    in
    case message of
        Tick _ ->
            ( model, Cmd.none )

        SessionLoaded (Ok payload) ->
            modelFromSessionPayload env payload

        SessionLoaded (Err error) ->
            ( Failed env, Cmd.none )

        LinkClicked (Browser.Internal url) ->
            ( model, Cmd.none )

        LinkClicked (Browser.External href) ->
            ( model, Nav.load href )

        UpdateHomeInput value ->
            case model of
                Authorized (Home homePage) _ session ->
                    let
                        nextHome =
                            { homePage | currentInput = value }
                    in
                    ( Authorized (Home nextHome) env session, Cmd.none )

                _ ->
                    ( model, Cmd.none )

        AttemptSend payload ->
            let
                ( nextModel, tick ) =
                    case model of
                        Authorized (Home homePage) _ session ->
                            let
                                newTick =
                                    homePage.tick + 1

                                nextHome =
                                    { homePage | lastRequest = Pending, tick = newTick }
                            in
                            ( Authorized (Home nextHome) env session, nextHome.tick )

                        _ ->
                            ( model, 0 )

                cmd =
                    sendInputMessage payload tick
            in
            ( nextModel, cmd )

        UrlChanged _ ->
            ( model, Cmd.none )

        WebsocketMessage rawMessage ->
            let
                parsed =
                    parseMessage rawMessage

                ( newModel, cmd ) =
                    case ( parsed, model ) of
                        ( Ok res, Authorized (Home home) _ session ) ->
                            let
                                connected =
                                    case ( res.kind == "control", res.connected ) of
                                        ( True, Just True ) ->
                                            True

                                        ( True, Nothing ) ->
                                            False

                                        ( True, Just False ) ->
                                            False

                                        ( False, _ ) ->
                                            home.connected

                                newHome =
                                    { home | lastRequest = Done (Ok ()), connected = connected }
                            in
                            ( Authorized (Home newHome) env session, Cmd.none )

                        ( Err _, Authorized (Home home) _ session ) ->
                            ( model, Cmd.none )

                        _ ->
                            ( model, Cmd.none )
            in
            ( newModel, cmd )


view : Model -> Browser.Document Message
view model =
    { title = "milton-ui"
    , body = [ viewBody model, viewFooter model ]
    }


subscriptions : Model -> Sub Message
subscriptions model =
    Sub.batch [ Time.every 1000 Tick, messageReceiver WebsocketMessage ]


envFromModel : Model -> Environment
envFromModel model =
    case model of
        Authorized _ env _ ->
            env

        Failed env ->
            env

        Booting env ->
            env

        Unauthorized env ->
            env


modelFromSessionPayload : Environment -> SessionPayload -> ( Model, Cmd Message )
modelFromSessionPayload env payload =
    case payload.ok of
        True ->
            ( Maybe.map (Authorized (Home emptyHome) env) payload.session.user
                |> Maybe.withDefault (Unauthorized env)
            , startWebsocket
            )

        False ->
            ( Unauthorized env, Cmd.none )


emptyHome : HomePage
emptyHome =
    { lastRequest = NotAsked, tick = 0, connected = False, currentInput = "" }


getAuthURL : Model -> String
getAuthURL model =
    (envFromModel model |> .apiRoot) ++ "/auth/identify"


loadAuth : Model -> Cmd Message
loadAuth model =
    Http.get { url = getAuthURL model, expect = Http.expectJson SessionLoaded sessionDecoder }


sessionUserDataDecoder : JD.Decoder SessionUserData
sessionUserDataDecoder =
    JD.map3 SessionUserData
        (JD.field "name" JD.string)
        (JD.field "picture" JD.string)
        (JD.field "user_id" JD.string)


sessionFieldDecoder : JD.Decoder SessionData
sessionFieldDecoder =
    JD.map SessionData (JD.nullable (JD.field "user" sessionUserDataDecoder))


sessionDecoder : JD.Decoder SessionPayload
sessionDecoder =
    JD.map2 SessionPayload
        (JD.field "ok" JD.bool)
        (JD.field "session" sessionFieldDecoder)


viewFooter : Model -> Html.Html Message
viewFooter model =
    Html.footer [ AT.class "footer fixed bottom-0 left-0 w-full bg-slate-800" ]
        [ Html.div [ AT.class "flex items-center px-3 py-2 border-t border-solid border-slate-800" ]
            [ Html.div [ AT.class "ml-auto" ]
                [ Html.a [ AT.href "https://github.com/dadleyy/milton", AT.rel "noopener", AT.target "_blank" ]
                    [ Html.text (envFromModel model |> .version) ]
                ]
            ]
        ]


viewBody : Model -> Html.Html Message
viewBody model =
    Html.div [ AT.class "w-full h-full relative pb-12" ]
        [ case model of
            Booting _ ->
                Html.div [ AT.class "relative w-full h-full flex items-center" ]
                    [ Html.div [ AT.class "mx-auto" ] [ Html.text "loading..." ]
                    ]

            Unauthorized _ ->
                Html.div [ AT.class "relative w-full h-full flex items-center" ]
                    [ Html.div [ AT.class "mx-auto" ]
                        [ Html.a
                            [ AT.href (envFromModel model |> .loginURL)
                            , AT.target "_self"
                            , AT.rel "noopener"
                            ]
                            [ Html.text "login" ]
                        ]
                    ]

            Authorized activePage env session ->
                Html.div [] [ header env session, viewPage activePage env session ]

            Failed _ ->
                Html.div [ AT.class "relative w-full h-full flex items-center justify-center" ]
                    [ Html.div [ AT.class "text-center" ]
                        [ Html.div []
                            [ Html.a
                                [ AT.href (envFromModel model |> .loginURL)
                                , AT.target "_self"
                                , AT.rel "noopener"
                                ]
                                [ Html.text "login" ]
                            ]
                        , Html.div [] [ Html.text "unable to load." ]
                        ]
                    ]
        ]


viewPage : Page -> Environment -> SessionUserData -> Html.Html Message
viewPage page env session =
    case page of
        Home homePage ->
            let
                isDisabled =
                    homePage.connected == False || homePage.lastRequest == Pending
            in
            Html.div [ AT.class "pt-8 flex items-center flex-col w-full h-full" ]
                [ Html.div [ AT.class "flex items-center w-full px-8" ]
                    [ Html.div [ AT.class "mr-4" ]
                        [ if homePage.connected then
                            Icon.view Icon.Wifi

                          else
                            Icon.view Icon.Exclamation
                        ]
                    , Html.input
                        [ AT.type_ "text"
                        , AT.class "mr-3 flex-1"
                        , AT.value homePage.currentInput
                        , EV.onInput UpdateHomeInput
                        , AT.disabled isDisabled
                        ]
                        []
                    , Button.view
                        ( Button.Icon Button.Plane (AttemptSend homePage.currentInput)
                        , if String.isEmpty homePage.currentInput || isDisabled then
                            Button.Disabled

                          else
                            Button.Primary
                        )
                    ]
                , Html.div [ AT.class "w-full flex-1 px-8 mt-4" ]
                    [ Html.div [ AT.class "code-container w-full" ]
                        [ Html.code []
                            [ Html.text "hi" ]
                        ]
                    ]
                ]


header : Environment -> SessionUserData -> Html.Html Message
header env session =
    Html.div [ AT.class "px-3 py-3 flex items-center border-b border-solid border-stone-700" ]
        [ Html.div []
            [ Html.div [] [ Html.text session.name ] ]
        , Html.div [ AT.class "ml-auto" ]
            [ Html.a [ AT.href (env |> .logoutURL), AT.target "_self", AT.rel "noopener" ]
                [ Html.text "logout" ]
            ]
        ]


startWebsocket : Cmd Message
startWebsocket =
    let
        message =
            JE.object [ ( "kind", JE.string "control" ) ]
    in
    sendMessage (JE.encode 0 message)


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
    sendMessage (JE.encode 0 values)


decoder : JD.Decoder WebsocketResponse
decoder =
    JD.map3 WebsocketResponse
        (JD.field "kind" JD.string)
        (JD.maybe (JD.field "connected" JD.bool))
        (JD.maybe (JD.field "payload" JD.string))


parseMessage : String -> Result JD.Error WebsocketResponse
parseMessage inner =
    JD.decodeString
        decoder
        inner
