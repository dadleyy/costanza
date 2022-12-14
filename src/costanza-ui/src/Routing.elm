module Routing exposing (..)

import Browser.Navigation as Nav
import Environment as Env
import HomePage
import Html
import Html.Attributes as AT
import Session
import Time
import Url


type Route
    = Home HomePage.HomePage
    | Loading


type Message
    = Tick Time.Posix
    | HomeMessage HomePage.Message
    | WebsocketMessage String


view : Route -> Html.Html Message
view route =
    case route of
        Loading ->
            Html.div [] []

        Home homePage ->
            Html.div [ AT.attribute "data-role" "route-container" ]
                [ HomePage.view homePage |> Html.map HomeMessage ]


authorized : Env.Environment -> Url.Url -> Nav.Key -> Session.SessionUserData -> ( Route, Cmd Message )
authorized env url key session =
    case String.startsWith (env.uiRoot ++ "home") url.path of
        True ->
            ( Home (HomePage.init env key url), Cmd.none )

        False ->
            ( Loading, Nav.replaceUrl key (env.uiRoot ++ "home/terminal") )


update : Env.Environment -> Nav.Key -> Message -> Route -> ( Route, Cmd Message )
update env key message route =
    case ( message, route ) of
        ( _, Loading ) ->
            ( route, Cmd.none )

        ( WebsocketMessage content, Home homePage ) ->
            updateHome (HomePage.RawWebsocket content) ( homePage, env, key )

        ( Tick posix, Home homePage ) ->
            updateHome (HomePage.Tick posix) ( homePage, env, key )

        ( HomeMessage homeMessage, Home homePage ) ->
            updateHome homeMessage ( homePage, env, key )


updateHome : HomePage.Message -> ( HomePage.HomePage, Env.Environment, Nav.Key ) -> ( Route, Cmd Message )
updateHome message ( homePage, env, key ) =
    let
        ( newInner, routeCmd ) =
            HomePage.update message ( homePage, env, key )
    in
    ( Home newInner, routeCmd |> Cmd.map HomeMessage )
