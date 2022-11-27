module Main exposing (..)

import Boot
import Browser
import Browser.Navigation as Nav
import Environment as Env
import Html
import Html.Attributes as AT
import Html.Events as EV
import Http
import Routing
import Session
import Task
import Time
import Url


type alias AuthorizedRoute =
    { env : Env.Environment
    , session : Session.SessionUserData
    , key : Nav.Key
    }


type Model
    = Booting Env.Environment
    | Unauthorized Env.Environment
    | Failed Env.Environment
      -- All routes handled by the routing module will require some user session data.
    | Authorized Routing.Route AuthorizedRoute


type Message
    = LinkClicked Browser.UrlRequest
    | UrlChanged Url.Url
    | RouteMessage Routing.Message
    | SessionLoaded Url.Url Nav.Key (Result Http.Error Session.SessionPayload)
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
    -- When our application boots, we'll start in the "Booting" state and immediately fire off an
    -- attempt to load the current session.
    ( Booting env, Cmd.batch [ loadAuth env url key ] )


update : Message -> Model -> ( Model, Cmd Message )
update message model =
    let
        env =
            envFromModel model
    in
    case ( message, model ) of
        ( RouteMessage innerMessage, Authorized innerModel shared ) ->
            let
                ( newRoute, routeCmd ) =
                    Routing.update env shared.key innerMessage innerModel
            in
            ( Authorized newRoute shared, routeCmd |> Cmd.map RouteMessage )

        ( WebsocketMessage websocketMessagePayload, Authorized innerModel shared ) ->
            let
                ( newRoute, routeCmd ) =
                    Routing.update env shared.key (Routing.WebsocketMessage websocketMessagePayload) innerModel
            in
            ( Authorized newRoute shared, routeCmd |> Cmd.map RouteMessage )

        ( Tick posixValue, Authorized innerModel shared ) ->
            let
                ( newRoute, routeCmd ) =
                    Routing.update env shared.key (Routing.Tick posixValue) innerModel
            in
            ( Authorized newRoute shared, routeCmd |> Cmd.map RouteMessage )

        ( UrlChanged url, Authorized innerModel shared ) ->
            let
                ( initialRoute, routeCmd ) =
                    Routing.authorized shared.env url shared.key shared.session
            in
            ( Authorized initialRoute shared, Cmd.batch [ Boot.startWebsocket, routeCmd |> Cmd.map RouteMessage ] )

        ( Tick posixValue, _ ) ->
            ( model, Cmd.none )

        -- If our session was loaded ok, we're going to want to start determining what we should be
        -- rendering based on the current url.
        ( SessionLoaded url key (Ok payload), _ ) ->
            case ( payload.ok, payload.session.user ) of
                ( True, Just sessionData ) ->
                    let
                        ( initialRoute, routeCmd ) =
                            Routing.authorized env url key sessionData
                    in
                    ( Authorized initialRoute { env = env, session = sessionData, key = key }
                    , Cmd.batch [ Boot.startWebsocket, routeCmd |> Cmd.map RouteMessage ]
                    )

                _ ->
                    ( Failed env, Cmd.none )

        -- If for whatever reason, our session fails, just leave the user on the current page and
        -- update our model so we render some catchall failed state.
        ( SessionLoaded _ _ (Err _), _ ) ->
            ( Failed env, Cmd.none )

        ( LinkClicked (Browser.Internal url), _ ) ->
            ( model, Cmd.none )

        ( LinkClicked (Browser.External href), _ ) ->
            ( model, Nav.load href )

        ( UrlChanged url, _ ) ->
            ( model, Cmd.none )

        -- TODO: should be unreachable - we received a message from a route but were not authorized.
        ( RouteMessage _, _ ) ->
            ( model, Cmd.none )

        -- TODO: should be unreachable - only authorized routes will be dealing with websocket messages.
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
    -- All variants of our model enum need to have an environment somewhere; it is effectively
    -- our global, configuration state.
    case model of
        Authorized _ shared ->
            shared.env

        Failed env ->
            env

        Booting env ->
            env

        Unauthorized env ->
            env


modelFromSessionPayload : Env.Environment -> Url.Url -> Nav.Key -> Session.SessionPayload -> ( Model, Cmd Message )
modelFromSessionPayload env url key payload =
    -- After we've loaded our initial session payload, start our application routing by returning
    -- a redirect to the home page.
    case ( payload.ok, payload.session.user ) of
        ( True, Just session ) ->
            let
                ( routeModel, routeCmd ) =
                    Routing.authorized env url key session
            in
            ( Authorized routeModel { env = env, session = session, key = key }, routeCmd |> Cmd.map RouteMessage )

        _ ->
            ( Unauthorized env, Cmd.none )


getAuthURL : Env.Environment -> String
getAuthURL env =
    env.apiRoot ++ "/auth/identify"


loadAuth : Env.Environment -> Url.Url -> Nav.Key -> Cmd Message
loadAuth env url key =
    -- As the very first thing we'll do, the auth request will attempt to load our identity. This needs
    -- to keep track of where we are when we made the request, and be able to redirect; the url and key
    -- being passed into our message handle that.
    Http.get { url = getAuthURL env, expect = Http.expectJson (SessionLoaded url key) Session.decode }


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

            Authorized activePage _ ->
                Routing.view activePage |> Html.map RouteMessage

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
