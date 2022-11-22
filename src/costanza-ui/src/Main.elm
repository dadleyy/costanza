module Main exposing (..)

import Boot
import Browser
import Browser.Navigation as Nav
import Environment as Env
import HomePage
import Html
import Html.Attributes as AT
import Html.Events as EV
import Http
import Session
import Task
import Time
import Url



-- The connection state represents that we are either disconnected from the websocket, or
-- we are connected with some connection state for the underlying serial connection.


type Page
    = Home HomePage.HomePage


type Model
    = Booting Env.Environment
    | Unauthorized Env.Environment
    | Authorized Page Env.Environment Session.SessionUserData
    | Failed Env.Environment


type Message
    = LinkClicked Browser.UrlRequest
    | UrlChanged Url.Url
    | HomeMessage HomePage.Message
    | SessionLoaded (Result Http.Error Session.SessionPayload)
    | WebsocketMessage String
    | Tick Time.Posix


main : Program Env.Environment Model Message
main =
    Browser.application
        { init = init
        , view = view
        , update = update
        , subscriptions = subscriptions
        , onUrlChange = UrlChanged
        , onUrlRequest = LinkClicked
        }


init : Env.Environment -> Url.Url -> Nav.Key -> ( Model, Cmd Message )
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
    case ( message, model ) of
        ( HomeMessage inner, Authorized (Home homePage) _ session ) ->
            let
                ( nextHome, cmd ) =
                    HomePage.update inner ( homePage, env )
            in
            ( Authorized (Home nextHome) env session, cmd |> Cmd.map HomeMessage )

        ( HomeMessage _, _ ) ->
            ( model, Cmd.none )

        ( Tick posixValue, Authorized (Home homePage) _ session ) ->
            let
                ( nextHome, cmd ) =
                    HomePage.update (HomePage.Tick posixValue) ( homePage, env )
            in
            ( Authorized (Home nextHome) env session, cmd |> Cmd.map HomeMessage )

        ( Tick posixValue, _ ) ->
            ( model, Cmd.none )

        ( SessionLoaded (Ok payload), _ ) ->
            modelFromSessionPayload env payload

        ( SessionLoaded (Err error), _ ) ->
            ( Failed env, Cmd.none )

        ( LinkClicked (Browser.Internal url), _ ) ->
            ( model, Cmd.none )

        ( LinkClicked (Browser.External href), _ ) ->
            ( model, Nav.load href )

        ( UrlChanged _, _ ) ->
            ( model, Cmd.none )

        ( WebsocketMessage rawMessage, Authorized (Home homePage) _ session ) ->
            let
                ( nextHome, cmd ) =
                    HomePage.update (HomePage.RawWebsocket rawMessage) ( homePage, env )
            in
            ( Authorized (Home nextHome) env session, cmd |> Cmd.map HomeMessage )

        ( WebsocketMessage _, _ ) ->
            ( model, Cmd.none )


view : Model -> Browser.Document Message
view model =
    { title = "costanza"
    , body = [ viewBody model, viewFooter model ]
    }


subscriptions : Model -> Sub Message
subscriptions model =
    Sub.batch [ Time.every 1000 Tick, Boot.messageReceiver WebsocketMessage ]


envFromModel : Model -> Env.Environment
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


modelFromSessionPayload : Env.Environment -> Session.SessionPayload -> ( Model, Cmd Message )
modelFromSessionPayload env payload =
    case payload.ok of
        True ->
            ( Maybe.map (Authorized (Home HomePage.init) env) payload.session.user
                |> Maybe.withDefault (Unauthorized env)
            , Boot.startWebsocket
            )

        False ->
            ( Unauthorized env, Cmd.none )


getAuthURL : Model -> String
getAuthURL model =
    (envFromModel model |> .apiRoot) ++ "/auth/identify"


loadAuth : Model -> Cmd Message
loadAuth model =
    Http.get { url = getAuthURL model, expect = Http.expectJson SessionLoaded Session.decode }


viewFooter : Model -> Html.Html Message
viewFooter model =
    Html.footer [ AT.class "footer fixed bottom-0 left-0 w-full bg-slate-800" ]
        [ Html.div [ AT.class "flex items-center px-3 py-2 border-t border-solid border-slate-800" ]
            [ Html.div [ AT.class "ml-auto" ]
                [ Html.a [ AT.href "https://github.com/dadleyy/costanza", AT.rel "noopener", AT.target "_blank" ]
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


viewPage : Page -> Env.Environment -> Session.SessionUserData -> Html.Html Message
viewPage page env session =
    case page of
        Home homePage ->
            HomePage.view homePage |> Html.map HomeMessage


header : Env.Environment -> Session.SessionUserData -> Html.Html Message
header env session =
    Html.div [ AT.class "px-3 py-3 flex items-center border-b border-solid border-stone-700" ]
        [ Html.div []
            [ Html.div [] [ Html.text session.name ] ]
        , Html.div [ AT.class "ml-auto" ]
            [ Html.a [ AT.href (env |> .logoutURL), AT.target "_self", AT.rel "noopener" ]
                [ Html.text "logout" ]
            ]
        ]
