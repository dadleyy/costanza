port module Main exposing (..)

import Browser
import Browser.Navigation as Nav
import Button
import Html
import Html.Attributes as AT
import Http
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
    }


type Page
    = Home HomePage
    | Input String


type Model
    = Booting Environment
    | Unauthorized Environment
    | Authorized Page Environment SessionUserData
    | Failed Environment


type Message
    = LinkClicked Browser.UrlRequest
    | UrlChanged Url.Url
    | SessionLoaded (Result Http.Error SessionPayload)
    | WebsocketMessage String
    | Tick Time.Posix


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

        UrlChanged _ ->
            ( model, Cmd.none )

        WebsocketMessage rawMessage ->
            Debug.log (Debug.toString rawMessage)
                ( model, Cmd.none )


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
            ( Maybe.map (Authorized (Home (HomePage NotAsked)) env) payload.session.user
                |> Maybe.withDefault (Unauthorized env)
            , startWebsocket
            )

        False ->
            ( Unauthorized env, Cmd.none )


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
            Html.div [ AT.class "pt-8 flex items-center flex-col w-full h-full" ]
                []

        Input value ->
            Html.div [ AT.class "px-3 py-3" ]
                []


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
